use obsidian_indexer::Database;

#[test]
fn fts_populated_from_chunks() {
    let path = std::env::temp_dir().join(format!("idx-{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);

    let mut db = Database::open(&path).expect("db");
    let id = db
        .upsert_file("notes/x.md", "md", 3, 0, "deadbeef")
        .expect("upsert");
    db.insert_chunks(id, &["hello world phrase".to_string()])
        .expect("chunks");

    let n: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM chunks_fts", [], |r| r.get(0))
        .expect("count fts");
    assert_eq!(n, 1);

    let matches: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM chunks_fts WHERE chunks_fts MATCH 'hello'",
            [],
            |r| r.get(0),
        )
        .expect("match");
    assert_eq!(matches, 1);
}
