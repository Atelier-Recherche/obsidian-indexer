use anyhow::Result;
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

fn strip_yaml_frontmatter(input: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\A---\s*\r?\n[\s\S]*?\r?\n---\s*\r?\n").expect("regex"));
    if let Some(m) = re.find(input) {
        input[m.end()..].to_string()
    } else {
        input.to_string()
    }
}

pub fn extract_markdown(path: &Path, strip_code_blocks: bool) -> Result<String> {
    let src = fs::read_to_string(path)?;
    let body = strip_yaml_frontmatter(&src);

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(&body, opts);
    let mut out = String::new();
    let mut skip_code = false;

    for ev in parser {
        match ev {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Indented | CodeBlockKind::Fenced(_))) => {
                skip_code = strip_code_blocks;
            }
            Event::End(TagEnd::CodeBlock) => {
                skip_code = false;
            }
            Event::Text(t) | Event::Code(t) => {
                if !skip_code || !strip_code_blocks {
                    out.push_str(&t);
                    out.push(' ');
                }
            }
            Event::SoftBreak | Event::HardBreak => out.push('\n'),
            _ => {}
        }
    }

    Ok(out)
}
