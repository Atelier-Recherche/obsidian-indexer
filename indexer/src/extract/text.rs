/// Split plain text into chunks suitable for FTS (length-bounded, UTF-8 safe).
pub fn chunk_text(input: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    let mut chunks = Vec::new();
    let mut buf = String::new();

    for para in input.split("\n\n") {
        let p = para.trim();
        if p.is_empty() {
            continue;
        }

        let candidate = if buf.is_empty() {
            p.to_string()
        } else {
            format!("{buf}\n\n{p}")
        };

        if candidate.len() <= max_chars {
            buf = candidate;
            continue;
        }

        if !buf.is_empty() {
            chunks.push(buf);
            buf = String::new();
        }

        if p.len() <= max_chars {
            buf = p.to_string();
        } else {
            chunks.extend(split_hard(p, max_chars));
        }
    }

    if !buf.is_empty() {
        chunks.push(buf);
    }

    if chunks.is_empty() && !input.trim().is_empty() {
        chunks.extend(split_hard(input.trim(), max_chars));
    }

    chunks
}

fn split_hard(s: &str, max_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    for ch in s.chars() {
        let next_len = buf.len() + ch.len_utf8();
        if next_len > max_chars && !buf.is_empty() {
            out.push(buf);
            buf = String::new();
        }
        buf.push(ch);
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_respect_max() {
        let s = "a".repeat(1000);
        let c = chunk_text(&s, 100);
        assert!(c.iter().all(|x| x.len() <= 100));
    }

    #[test]
    fn merges_short_paragraphs() {
        let s = "one\n\ntwo\n\nthree";
        let c = chunk_text(s, 100);
        assert_eq!(c.len(), 1);
    }
}
