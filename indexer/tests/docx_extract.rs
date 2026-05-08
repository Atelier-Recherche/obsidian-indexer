use obsidian_indexer::extract::extract_docx;
use std::fs::File;
use std::io::Write;
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

#[test]
fn docx_regex_extracts_body() {
    let path = std::env::temp_dir().join(format!("sample-{}.docx", std::process::id()));
    let file = File::create(&path).unwrap();
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("word/document.xml", opts)
    .unwrap();
    zip
        .write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body><w:p><w:r><w:t>HelloDocxWord</w:t></w:r></w:p></w:body>
</w:document>"#,
        )
        .unwrap();
    zip.finish().unwrap();

    let s = extract_docx(&path).unwrap();
    assert!(s.contains("HelloDocxWord"));
}
