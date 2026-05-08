use crate::db::Database;
use crate::extract::{self, extract_path};
use crate::walk::{walk_vault, FileKind};
use anyhow::{Context, Result};
use rayon::prelude::*;
use rusqlite::OptionalExtension;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct IndexConfig {
    pub vault_root: PathBuf,
    pub strip_code_blocks: bool,
    pub max_chars_per_chunk: usize,
}

pub struct IndexStats {
    pub indexed: usize,
    pub skipped: usize,
    pub removed: usize,
    pub discovered: usize,
    pub indexed_by_kind: FileKindCounts,
    pub skipped_by_kind: FileKindCounts,
    pub extraction_failed_by_kind: FileKindCounts,
    pub pdf_annotations_indexed: usize,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FileKindCounts {
    pub md: usize,
    pub docx: usize,
    pub pdf: usize,
    pub epub: usize,
}

impl FileKindCounts {
    fn bump(&mut self, kind: FileKind) {
        match kind {
            FileKind::Markdown => self.md += 1,
            FileKind::Docx => self.docx += 1,
            FileKind::Pdf => self.pdf += 1,
            FileKind::Epub => self.epub += 1,
        }
    }
}

fn rel_path_key(vault: &Path, abs: &Path) -> Result<String> {
    let rel = abs.strip_prefix(vault).with_context(|| {
        format!("path not under vault: {} / {}", vault.display(), abs.display())
    })?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

fn mtime_unix(path: &Path) -> Result<i64> {
    let meta = fs::metadata(path)?;
    let t = meta.modified().unwrap_or_else(|_| SystemTime::now());
    let d = t.duration_since(UNIX_EPOCH).unwrap_or_default();
    Ok(d.as_secs() as i64)
}

struct PreparedFile {
    rel: String,
    kind: FileKind,
    size: u64,
    mtime: i64,
    hash: String,
}

pub fn index_vault(db: &mut Database, cfg: &IndexConfig) -> Result<IndexStats> {
    let paths = walk_vault(&cfg.vault_root)?;
    let vault_paths: HashSet<String> = paths
        .iter()
        .map(|p| rel_path_key(&cfg.vault_root, p))
        .collect::<Result<_>>()?;

    let indexed_snapshot = db.list_indexed_paths()?;
    let mut removed = 0usize;
    for old in indexed_snapshot {
        if !vault_paths.contains(&old) {
            db.delete_file_by_path(&old)?;
            removed += 1;
        }
    }

    let prepared: Vec<PreparedFile> = paths
        .par_iter()
        .map(|path| -> Result<PreparedFile> {
            let kind =
                FileKind::from_path(path).with_context(|| format!("unknown kind {:?}", path))?;
            let rel = rel_path_key(&cfg.vault_root, path)?;
            let size = fs::metadata(path)?.len();
            let mtime = mtime_unix(path)?;
            let hash = extract::hash_file(path)?;
            Ok(PreparedFile {
                rel,
                kind,
                size,
                mtime,
                hash,
            })
        })
        .collect::<Result<_>>()?;

    let mut indexed = 0usize;
    let mut skipped = 0usize;
    let mut indexed_by_kind = FileKindCounts::default();
    let mut skipped_by_kind = FileKindCounts::default();
    let mut extraction_failed_by_kind = FileKindCounts::default();
    let mut pdf_annotations_indexed = 0usize;

    for p in prepared {
        let existing: Option<String> = db
            .conn()
            .query_row(
                "SELECT content_hash FROM files WHERE vault_rel_path = ?1",
                rusqlite::params![p.rel],
                |r| r.get::<_, String>(0),
            )
            .optional()?;

        if existing.as_deref() == Some(p.hash.as_str()) {
            skipped += 1;
            skipped_by_kind.bump(p.kind);
            continue;
        }

        let abs: PathBuf = p
            .rel
            .split('/')
            .filter(|s| !s.is_empty())
            .fold(cfg.vault_root.clone(), |a, s| a.join(s));
        let extracted = match extract_path(
            &abs,
            p.kind,
            cfg.strip_code_blocks,
            cfg.max_chars_per_chunk,
        ) {
            Ok(c) => c,
            Err(e) => {
                let msg = format!("{e:#}");
                if msg.contains("Pdfium introuvable") {
                    tracing::debug!(path = %p.rel, "PDF ignoré (Pdfium)");
                } else {
                    tracing::warn!(path = %p.rel, error = ?e, "extraction failed; index entry empty");
                }
                extraction_failed_by_kind.bump(p.kind);
                crate::extract::ExtractResult {
                    chunks: Vec::new(),
                    pdf_annotations: 0,
                }
            }
        };
        pdf_annotations_indexed += extracted.pdf_annotations;

        let file_id = db.upsert_file(
            &p.rel,
            p.kind.as_str(),
            p.size as i64,
            p.mtime,
            &p.hash,
        )?;
        db.clear_chunks_for_file(file_id)?;
        if !extracted.chunks.is_empty() {
            db.insert_chunks(file_id, &extracted.chunks)?;
        }
        indexed += 1;
        indexed_by_kind.bump(p.kind);
    }

    Ok(IndexStats {
        indexed,
        skipped,
        removed,
        discovered: paths.len(),
        indexed_by_kind,
        skipped_by_kind,
        extraction_failed_by_kind,
        pdf_annotations_indexed,
    })
}
