use anyhow::Result;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileKind {
    Markdown,
    Docx,
    Pdf,
    Epub,
}

impl FileKind {
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "md" | "markdown" => Some(FileKind::Markdown),
            "docx" => Some(FileKind::Docx),
            "pdf" => Some(FileKind::Pdf),
            "epub" => Some(FileKind::Epub),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            FileKind::Markdown => "md",
            FileKind::Docx => "docx",
            FileKind::Pdf => "pdf",
            FileKind::Epub => "epub",
        }
    }
}

fn is_under_obsidian_ignore(rel: &Path) -> bool {
    rel.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == ".obsidian" || s == ".git" || s == ".cursor" || s == ".stversions"
    })
}

fn has_hidden_component(rel: &Path) -> bool {
    rel.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s.starts_with('.') && s != "." && s != ".."
    })
}

/// Enumerate indexable files under vault root using gitignore-style rules when present.
pub fn walk_vault(vault_root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let walker = WalkBuilder::new(vault_root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .parents(true)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(rel) = path.strip_prefix(vault_root) else {
            continue;
        };
        if is_under_obsidian_ignore(rel) {
            continue;
        }
        if has_hidden_component(rel) {
            continue;
        }
        if FileKind::from_path(path).is_some() {
            out.push(path.to_path_buf());
        }
    }

    out.sort();
    Ok(out)
}
