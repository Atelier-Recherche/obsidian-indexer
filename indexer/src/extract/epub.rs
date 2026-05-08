use anyhow::Result;
use epub::doc::EpubDoc;
use html_escape::decode_html_entities;
use std::path::Path;

pub fn extract_epub(path: &Path) -> Result<String> {
    let mut doc = EpubDoc::new(path)
        .map_err(|e| anyhow::anyhow!("epub {:?}: {e:?}", path))?;
    let mut out = String::new();

    loop {
        if let Some((html, _mime)) = doc.get_current_str() {
            let text = strip_html_tags(&html);
            let text = decode_html_entities(&text).to_string();
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !text.is_empty() {
                out.push_str(&text);
                out.push('\n');
            }
        } else {
            break;
        }
        if !doc.go_next() {
            break;
        }
    }

    Ok(out)
}

fn strip_html_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}
