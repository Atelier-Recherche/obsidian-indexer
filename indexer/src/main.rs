use anyhow::Result;
use clap::Parser;
use obsidian_indexer::{index_vault, Database, IndexConfig};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Parser)]
#[command(name = "obsidian-indexer", about = "Index an Obsidian vault into SQLite (FTS4)")]
struct Cli {
    /// Root path of the vault (folder containing notes)
    #[arg(value_name = "VAULT")]
    vault: PathBuf,

    /// SQLite database path (relative paths are resolved under VAULT)
    #[arg(long, default_value = ".obsidian-index/index.sqlite")]
    db: PathBuf,

    /// Omit fenced / indented code blocks when indexing Markdown
    #[arg(long, default_value_t = false)]
    strip_code_blocks: bool,

    #[arg(long, default_value_t = 8192)]
    max_chunk_chars: usize,

    /// Re-run indexing when vault files change (debounced)
    #[arg(long, default_value_t = false)]
    watch: bool,
}

fn resolve_db(vault: &Path, db: &Path) -> PathBuf {
    if db.is_absolute() {
        db.to_path_buf()
    } else {
        vault.join(db)
    }
}

fn run_once(db_path: &Path, cfg: &IndexConfig) -> Result<()> {
    let mut db = Database::open(db_path)?;
    let s = index_vault(&mut db, cfg)?;
    println!(
        "indexed {} files, skipped {}, removed {}, discovered {}",
        s.indexed, s.skipped, s.removed, s.discovered
    );
    println!(
        "indexed_by_kind md={} docx={} pdf={} epub={} | skipped_by_kind md={} docx={} pdf={} epub={} | extraction_failed md={} docx={} pdf={} epub={} | pdf_annotations={}",
        s.indexed_by_kind.md,
        s.indexed_by_kind.docx,
        s.indexed_by_kind.pdf,
        s.indexed_by_kind.epub,
        s.skipped_by_kind.md,
        s.skipped_by_kind.docx,
        s.skipped_by_kind.pdf,
        s.skipped_by_kind.epub,
        s.extraction_failed_by_kind.md,
        s.extraction_failed_by_kind.docx,
        s.extraction_failed_by_kind.pdf,
        s.extraction_failed_by_kind.epub,
        s.pdf_annotations_indexed
    );
    Ok(())
}

#[cfg(feature = "watch")]
fn watch_loop(vault: PathBuf, db_path: PathBuf, cfg: IndexConfig) -> Result<()> {
    use notify::{RecursiveMode, Watcher};
    use std::sync::mpsc::channel;

    run_once(&db_path, &cfg)?;

    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    })?;

    watcher.watch(&vault, RecursiveMode::Recursive)?;
    eprintln!("watching {:?} (Ctrl+C to stop)", vault);

    while rx.recv().is_ok() {
        std::thread::sleep(Duration::from_secs(2));
        while rx.try_recv().is_ok() {}
        let _ = run_once(&db_path, &cfg);
    }

    Ok(())
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let db_path = resolve_db(&cli.vault, &cli.db);

    let cfg = IndexConfig {
        vault_root: cli.vault.clone(),
        strip_code_blocks: cli.strip_code_blocks,
        max_chars_per_chunk: cli.max_chunk_chars,
    };

    if cli.watch {
        #[cfg(feature = "watch")]
        {
            watch_loop(cli.vault, db_path, cfg)?;
        }
        #[cfg(not(feature = "watch"))]
        {
            anyhow::bail!("this binary was built without the `watch` feature");
        }
    } else {
        run_once(&db_path, &cfg)?;
    }

    Ok(())
}
