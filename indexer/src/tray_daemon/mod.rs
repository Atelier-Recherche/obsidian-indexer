//! Icône dans la zone de notification / menu contextuel.

mod config;
mod ui;

pub use config::TrayConfig;

use crate::{index_vault, Database, IndexConfig, IndexStats};
use anyhow::Result;
use config::resolve_db;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

pub fn run() -> Result<()> {
    init_tracing();

    let enabled = Arc::new(AtomicBool::new(
        TrayConfig::load_or_default().start_enabled,
    ));

    let worker_on = Arc::clone(&enabled);
    thread::spawn(move || worker_loop(worker_on));

    ui::run_tray_app(enabled)
}

/// Fenêtre egui uniquement — à lancer dans **un processus séparé** (`--config-gui`) pour éviter le conflit winit/Tao sur Windows.
pub fn run_config_gui_standalone() -> Result<()> {
    init_tracing();

    ui::run_config_standalone()
}

pub fn run_logs_gui_standalone() -> Result<()> {
    init_tracing();
    ui::run_logs_standalone()
}

pub fn run_status_gui_standalone() -> Result<()> {
    init_tracing();
    ui::run_status_standalone()
}

fn init_tracing() {
    let make_writer = BoxMakeWriter::new(move || {
        let mut writers: Vec<Box<dyn Write + Send>> = vec![Box::new(std::io::stdout())];
        let log_path = config::log_file_path();
        if let Some(dir) = log_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(f) = OpenOptions::new().create(true).append(true).open(log_path) {
            writers.push(Box::new(f));
        }
        Box::new(std::io::LineWriter::new(std::io::BufWriter::new(MultiWriter {
            writers,
        }))) as Box<dyn Write + Send>
    });

    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_ansi(false)
        .with_writer(make_writer)
        .try_init();
}

struct MultiWriter {
    writers: Vec<Box<dyn Write + Send>>,
}

impl Write for MultiWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for w in &mut self.writers {
            let _ = w.write_all(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        for w in &mut self.writers {
            let _ = w.flush();
        }
        Ok(())
    }
}

fn worker_loop(on: Arc<AtomicBool>) {
    loop {
        if !on.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(400));
            continue;
        }

        let snapshot = TrayConfig::load_or_default();
        let force_rebuild_path = config::force_rebuild_flag_path();

        let vault = PathBuf::from(&snapshot.vault_path);
        if snapshot.vault_path.is_empty() || !vault.is_dir() {
            tracing::warn!("vault_path invalide ou vide — configure via le menu (clic droit)");
            thread::sleep(Duration::from_secs(5));
            continue;
        }

        let db_path = resolve_db(&vault, Path::new(&snapshot.db_relative));
        if force_rebuild_path.exists() {
            tracing::info!("rebuild forcé demandé");
            match std::fs::remove_file(&db_path) {
                Ok(()) => tracing::info!("base supprimée (rebuild forcé) : {}", db_path.display()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!("suppression DB (rebuild forcé) : {}", e),
            }
            let _ = std::fs::remove_file(&force_rebuild_path);
        }
        let index_cfg = IndexConfig {
            vault_root: vault.clone(),
            strip_code_blocks: snapshot.strip_code_blocks,
            max_chars_per_chunk: snapshot.max_chunk_chars,
        };

        match run_once_quiet(&db_path, &index_cfg) {
            Ok(stats) => {
                tracing::info!(
                    indexed = stats.indexed,
                    skipped = stats.skipped,
                    removed = stats.removed,
                    discovered = stats.discovered,
                    indexed_md = stats.indexed_by_kind.md,
                    indexed_docx = stats.indexed_by_kind.docx,
                    indexed_pdf = stats.indexed_by_kind.pdf,
                    indexed_epub = stats.indexed_by_kind.epub,
                    skipped_md = stats.skipped_by_kind.md,
                    skipped_docx = stats.skipped_by_kind.docx,
                    skipped_pdf = stats.skipped_by_kind.pdf,
                    skipped_epub = stats.skipped_by_kind.epub,
                    failed_md = stats.extraction_failed_by_kind.md,
                    failed_docx = stats.extraction_failed_by_kind.docx,
                    failed_pdf = stats.extraction_failed_by_kind.pdf,
                    failed_epub = stats.extraction_failed_by_kind.epub,
                    pdf_annotations = stats.pdf_annotations_indexed,
                    "index pass"
                );
            }
            Err(e) => tracing::error!("indexation : {:#}", e),
        }

        let interval = snapshot.interval_seconds.max(10);
        let slices = interval.saturating_mul(10);
        for _ in 0..slices {
            if !on.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
}

fn run_once_quiet(db_path: &std::path::Path, cfg: &IndexConfig) -> Result<IndexStats> {
    let mut db = Database::open(db_path)?;
    index_vault(&mut db, cfg)
}
