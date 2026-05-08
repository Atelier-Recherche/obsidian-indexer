mod docx;
mod epub;
mod markdown;
mod pdf;
mod text;

pub use docx::extract_docx;
pub use epub::extract_epub;
pub use markdown::extract_markdown;
pub use pdf::extract_pdf;
pub use text::chunk_text;

use crate::walk::FileKind;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub struct ExtractResult {
    pub chunks: Vec<String>,
    pub pdf_annotations: usize,
}

/// Read file, detect kind from path, return UTF-8 text for indexing.
pub fn extract_path(
    path: &Path,
    kind: FileKind,
    strip_code_blocks: bool,
    max_chars_per_chunk: usize,
) -> Result<ExtractResult> {
    let raw = extract_raw(path, kind, strip_code_blocks)?;
    let pdf_annotations = if kind == FileKind::Pdf {
        raw.match_indices("[[ANNOTATION]]").count()
    } else {
        0
    };
    Ok(ExtractResult {
        chunks: crate::extract::chunk_text(&raw, max_chars_per_chunk),
        pdf_annotations,
    })
}

fn extract_raw(path: &Path, kind: FileKind, strip_code_blocks: bool) -> Result<String> {
    match kind {
        FileKind::Markdown => extract_markdown(path, strip_code_blocks),
        FileKind::Docx => extract_docx(path),
        FileKind::Epub => extract_epub(path),
        FileKind::Pdf => extract_pdf(path),
    }
}

/// Raw bytes hash for change detection (Blake3 hex).
pub fn hash_file(path: &Path) -> Result<String> {
    let data = fs::read(path)?;
    Ok(blake3::hash(&data).to_hex().to_string())
}
