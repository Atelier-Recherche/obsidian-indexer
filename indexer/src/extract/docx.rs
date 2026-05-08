use anyhow::{Context, Result};
use html_escape::decode_html_entities;
use regex::Regex;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;

fn wt_tag_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?s)<w:t[^>]*>(.*?)</w:t>").expect("regex"))
}

pub fn extract_docx(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("open {:?}", path))?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut xml = String::new();
    archive
        .by_name("word/document.xml")
        .with_context(|| "word/document.xml missing")?
        .read_to_string(&mut xml)?;

    let mut out = String::new();
    for cap in wt_tag_re().captures_iter(&xml) {
        let inner = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let inner = decode_html_entities(inner.trim());
        if !inner.is_empty() {
            out.push_str(&inner);
            out.push(' ');
        }
    }

    Ok(out)
}
