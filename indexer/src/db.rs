use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

const SCHEMA_VERSION: &str = "2";

/// Normalized SQLite + FTS4 (external content) for chunk search.
///
/// FTS5 n’est pas présent dans le build **sql.js** npm (navigateur / Obsidian) ; FTS4 est inclus
/// et suffit pour `MATCH` + `snippet` dans le plugin.
pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        std::fs::create_dir_all(
            path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| std::path::Path::new(".")),
        )
        .with_context(|| format!("create_dir_all {:?}", path.parent()))?;

        let conn = Connection::open(path).with_context(|| format!("open db {:?}", path))?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            ",
        )?;

        let mut db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }

    fn init_schema(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY,
                vault_rel_path TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                mtime_unix INTEGER NOT NULL,
                content_hash TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS chunks (
                chunk_id INTEGER PRIMARY KEY,
                file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                ordinal INTEGER NOT NULL,
                body TEXT NOT NULL
            );
            ",
        )?;

        let ver: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()?;

        match ver.as_deref() {
            None => {
                self.conn.execute_batch(Self::fts4_schema_sql())?;
                self.conn.execute(
                    "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)",
                    params![SCHEMA_VERSION],
                )?;
            }
            Some("1") => self.migrate_fts5_index_to_fts4()?,
            Some(SCHEMA_VERSION) => {
                self.conn.execute_batch(Self::fts4_schema_sql())?;
            }
            Some(v) => {
                anyhow::bail!("unsupported schema_version in DB: {v} (expected {SCHEMA_VERSION})");
            }
        }

        Ok(())
    }

    fn fts4_schema_sql() -> &'static str {
        // FTS4 sans table « external content » : évite les soucis de paramètres sqlite/sql.js ;
        // le corps du chunk est dupliqué dans l’index (taille DB un peu plus grande).
        r"
            CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts4(
                body,
                tokenize=unicode61
            );

            CREATE TRIGGER IF NOT EXISTS chunks_ai AFTER INSERT ON chunks BEGIN
                INSERT INTO chunks_fts(docid, body) VALUES (new.chunk_id, new.body);
            END;

            CREATE TRIGGER IF NOT EXISTS chunks_ad AFTER DELETE ON chunks BEGIN
                INSERT INTO chunks_fts(chunks_fts, docid, body) VALUES('delete', old.chunk_id, old.body);
            END;

            CREATE TRIGGER IF NOT EXISTS chunks_au AFTER UPDATE ON chunks BEGIN
                INSERT INTO chunks_fts(chunks_fts, docid, body) VALUES('delete', old.chunk_id, old.body);
                INSERT INTO chunks_fts(docid, body) VALUES (new.chunk_id, new.body);
            END;
        "
    }

    fn migrate_fts5_index_to_fts4(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "
            DROP TRIGGER IF EXISTS chunks_ai;
            DROP TRIGGER IF EXISTS chunks_ad;
            DROP TRIGGER IF EXISTS chunks_au;
            DROP TABLE IF EXISTS chunks_fts;
            ",
        )?;
        self.conn.execute_batch(Self::fts4_schema_sql())?;
        self.conn.execute_batch(
            "
            INSERT INTO chunks_fts(docid, body) SELECT chunk_id, body FROM chunks;
            ",
        )?;
        self.conn.execute(
            "UPDATE meta SET value = ?1 WHERE key = 'schema_version'",
            params![SCHEMA_VERSION],
        )?;
        Ok(())
    }

    pub fn file_id_by_path(&self, vault_rel_path: &str) -> Result<Option<i64>> {
        let id = self
            .conn
            .query_row(
                "SELECT id FROM files WHERE vault_rel_path = ?1",
                params![vault_rel_path],
                |r| r.get::<_, i64>(0),
            )
            .optional()?;
        Ok(id)
    }

    pub fn upsert_file(
        &mut self,
        vault_rel_path: &str,
        kind: &str,
        size_bytes: i64,
        mtime_unix: i64,
        content_hash: &str,
    ) -> Result<i64> {
        let path = vault_rel_path.to_string();
        let kind = kind.to_string();
        let hash = content_hash.to_string();

        self.conn.execute(
            "INSERT INTO files (vault_rel_path, kind, size_bytes, mtime_unix, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(vault_rel_path) DO UPDATE SET
               kind = excluded.kind,
               size_bytes = excluded.size_bytes,
               mtime_unix = excluded.mtime_unix,
               content_hash = excluded.content_hash",
            params![path, kind, size_bytes, mtime_unix, hash],
        )?;

        let id = self.conn.query_row(
            "SELECT id FROM files WHERE vault_rel_path = ?1",
            params![vault_rel_path],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    /// Remove all chunks for a file (triggers FTS delete), then optionally remove file row.
    pub fn clear_chunks_for_file(&mut self, file_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM chunks WHERE file_id = ?1", params![file_id])?;
        Ok(())
    }

    pub fn delete_file_by_path(&mut self, vault_rel_path: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM files WHERE vault_rel_path = ?1",
            params![vault_rel_path],
        )?;
        Ok(())
    }

    pub fn insert_chunks(&mut self, file_id: i64, bodies: &[String]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut stmt =
                tx.prepare_cached("INSERT INTO chunks (file_id, ordinal, body) VALUES (?1, ?2, ?3)")?;
            for (ord, body) in bodies.iter().enumerate() {
                stmt.execute(params![file_id, ord as i64, body.as_str()])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_indexed_paths(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT vault_rel_path FROM files ORDER BY vault_rel_path")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!(e))
    }
}
