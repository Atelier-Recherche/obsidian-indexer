use obsidian_indexer::{index_vault, Database, IndexConfig};
use std::fs;
use std::io::Write;

#[test]
fn index_vault_finds_markdown_content() {
    let root = std::env::temp_dir().join(format!("vault-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("notes")).unwrap();

    let mut f = fs::File::create(root.join("notes/hello.md")).unwrap();
    writeln!(f, "---\ntitle: T\n---\nunique_alpha_beta_gamma_token").unwrap();

    let db_path = root.join("index.sqlite");
    let cfg = IndexConfig {
        vault_root: root.clone(),
        strip_code_blocks: false,
        max_chars_per_chunk: 8192,
    };

    let mut db = Database::open(&db_path).unwrap();
    let s = index_vault(&mut db, &cfg).unwrap();
    assert!(s.indexed >= 1);

    let hits: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'unique_alpha_beta_gamma_token'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
}
