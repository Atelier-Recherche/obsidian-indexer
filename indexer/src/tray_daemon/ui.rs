//! Tao + tray-icon. La fenêtre egui (`--config-gui`) vit dans **un autre processus** :
//! une seule boucle winit par processus (obligation Windows).

use crate::tray_daemon::config::{self, TrayConfig};
use anyhow::Result;
use std::fs;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use image::ImageReader;
use rfd::FileDialog;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
};
#[cfg(windows)]
use winreg::enums::HKEY_CURRENT_USER;
#[cfg(windows)]
use winreg::RegKey;

#[derive(Clone, Debug)]
enum AppMsg {
    Tray(TrayIconEvent),
    Menu(MenuEvent),
}

pub fn run_tray_app(enabled: Arc<AtomicBool>) -> Result<()> {
    let event_loop = EventLoopBuilder::<AppMsg>::with_user_event().build();

    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |e| {
        let _ = proxy.send_event(AppMsg::Tray(e));
    }));

    let proxy_m = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = proxy_m.send_event(AppMsg::Menu(e));
    }));

    let (icon_rgba, w, h) = load_tray_icon_rgba()
        .unwrap_or_else(|_| (build_icon_rgba(), 32, 32));
    let icon = Icon::from_rgba(icon_rgba, w, h).map_err(|e| anyhow::anyhow!("{}", e))?;

    let enabled_btn = Arc::clone(&enabled);

    let tray_menu = Menu::new();
    let item_config = MenuItem::with_id("cfg", "Configuration…", true, None);
    let id_menu_config = item_config.id().clone();
    let item_status = MenuItem::with_id("status", "Bilan…", true, None);
    let id_menu_status = item_status.id().clone();
    let item_logs = MenuItem::with_id("logs", "Logs…", true, None);
    let id_menu_logs = item_logs.id().clone();
    let item_quit = MenuItem::with_id("quit", "Quitter", true, None);
    let id_menu_quit = item_quit.id().clone();
    if let Err(e) = tray_menu.append_items(&[&item_config, &item_status, &item_logs, &item_quit]) {
        return Err(anyhow::anyhow!("{}", e));
    }

    let mut tray_icon_slot: Option<tray_icon::TrayIcon> = None;

    event_loop.run(move |event, _elwt, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(StartCause::Init) => {
                let builder = TrayIconBuilder::new()
                    .with_tooltip(tooltip_for(enabled_btn.load(Ordering::SeqCst)))
                    .with_icon(icon.clone())
                    .with_menu(Box::new(tray_menu.clone()))
                    .with_menu_on_left_click(false)
                    .with_menu_on_right_click(true);

                match builder.build() {
                    Ok(ic) => tray_icon_slot = Some(ic),
                    Err(e) => eprintln!("obsidian-indexer-tray : icône {}", e),
                };
            }

            Event::UserEvent(AppMsg::Tray(ev)) => {
                if let TrayIconEvent::Click {
                    button,
                    button_state,
                    ..
                } = ev
                {
                    if button_state != MouseButtonState::Down {
                        return;
                    }
                    if button == MouseButton::Left {
                        let cur = !enabled_btn.load(Ordering::SeqCst);
                        enabled_btn.store(cur, Ordering::SeqCst);
                        let mut c = TrayConfig::load_or_default();
                        c.start_enabled = cur;
                        let _ = c.save();
                        if let Some(ref ti) = tray_icon_slot {
                            let _ = ti.set_tooltip(Some(tooltip_for(cur)));
                        }
                    }
                }
            }

            Event::UserEvent(AppMsg::Menu(ev)) => {
                if ev.id == id_menu_config {
                    spawn_config_gui_process();
                } else if ev.id == id_menu_status {
                    spawn_status_gui_process();
                } else if ev.id == id_menu_logs {
                    spawn_logs_gui_process();
                } else if ev.id == id_menu_quit {
                    tray_icon_slot.take();
                    *control_flow = ControlFlow::Exit;
                }
            }

            Event::LoopDestroyed => {
                tray_icon_slot.take();
            }

            _ => {}
        }
    });
}

fn spawn_config_gui_process() {
    match std::env::current_exe() {
        Ok(exe) => {
            if let Err(e) = std::process::Command::new(exe).arg("--config-gui").spawn() {
                tracing::error!("impossible de lancer la fenêtre de configuration : {}", e);
            }
        }
        Err(e) => tracing::error!("current_exe : {}", e),
    }
}

fn spawn_logs_gui_process() {
    match std::env::current_exe() {
        Ok(exe) => {
            if let Err(e) = std::process::Command::new(exe).arg("--logs-gui").spawn() {
                tracing::error!("impossible de lancer la fenêtre de logs : {}", e);
            }
        }
        Err(e) => tracing::error!("current_exe : {}", e),
    }
}

fn spawn_status_gui_process() {
    match std::env::current_exe() {
        Ok(exe) => {
            if let Err(e) = std::process::Command::new(exe).arg("--status-gui").spawn() {
                tracing::error!("impossible de lancer la fenêtre de bilan : {}", e);
            }
        }
        Err(e) => tracing::error!("current_exe : {}", e),
    }
}

/// Fenêtre egui — **processus dédié** uniquement (main thread = thread winit).
pub fn run_config_standalone() -> Result<()> {
    let shared = Arc::new(std::sync::Mutex::new(TrayConfig::load_or_default()));
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([520.0, 420.0])
            .with_title("Obsidian indexer — configuration"),
        ..Default::default()
    };

    let shared_c = Arc::clone(&shared);
    eframe::run_native(
        "Configuration",
        opts,
        Box::new(move |cc| {
            Ok(Box::new(ConfigPanel::new_standalone(cc, shared_c)) as Box<dyn eframe::App>)
        }),
    )
    .map_err(|e| anyhow::anyhow!("{:?}", e))
}

pub fn run_logs_standalone() -> Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 620.0])
            .with_title("Obsidian indexer — logs"),
        ..Default::default()
    };

    eframe::run_native(
        "Logs",
        opts,
        Box::new(move |_cc| Ok(Box::new(LogsPanel::default()) as Box<dyn eframe::App>)),
    )
    .map_err(|e| anyhow::anyhow!("{:?}", e))
}

pub fn run_status_standalone() -> Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 520.0])
            .with_title("Obsidian indexer — bilan"),
        ..Default::default()
    };

    eframe::run_native(
        "Bilan",
        opts,
        Box::new(move |_cc| Ok(Box::new(StatusPanel::default()) as Box<dyn eframe::App>)),
    )
    .map_err(|e| anyhow::anyhow!("{:?}", e))
}

fn tooltip_for(active: bool) -> String {
    if active {
        "Obsidian indexer — ACTIF (clic gauche : pause)".into()
    } else {
        "Obsidian indexer — PAUSE (clic gauche : démarrer)".into()
    }
}

fn build_icon_rgba() -> Vec<u8> {
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let i = ((y * 32 + x) * 4) as usize;
            let cx = x as f32 - 15.5;
            let cy = y as f32 - 15.5;
            if cx * cx + cy * cy < 14.0 * 14.0 {
                rgba[i] = 95;
                rgba[i + 1] = 135;
                rgba[i + 2] = 210;
                rgba[i + 3] = 255;
            }
        }
    }
    rgba
}

fn load_tray_icon_rgba() -> Result<(Vec<u8>, u32, u32)> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("tray-icon.png"));
        }
    }
    candidates.push(config::control_dir_path().join("tray-icon.png"));

    for p in candidates {
        if !p.exists() {
            continue;
        }
        let img = ImageReader::open(&p)
            .map_err(|e| anyhow::anyhow!("open icon {}: {}", p.display(), e))?
            .decode()
            .map_err(|e| anyhow::anyhow!("decode icon {}: {}", p.display(), e))?
            .to_rgba8();
        let (w, h) = img.dimensions();
        tracing::info!("icône tray chargée : {} ({}x{})", p.display(), w, h);
        return Ok((img.into_raw(), w, h));
    }

    anyhow::bail!("aucune icône tray personnalisée trouvée")
}

struct ConfigPanel {
    draft: TrayConfig,
    shared: Arc<std::sync::Mutex<TrayConfig>>,
    err: Option<String>,
}

struct LogsPanel {
    content: String,
    last_refresh: Instant,
    err: Option<String>,
    last_index_pass: Option<String>,
}

#[derive(Default, Clone)]
struct IndexPassMetrics {
    discovered: usize,
    indexed: usize,
    skipped: usize,
    removed: usize,
    indexed_md: usize,
    indexed_docx: usize,
    indexed_pdf: usize,
    indexed_epub: usize,
    skipped_md: usize,
    skipped_docx: usize,
    skipped_pdf: usize,
    skipped_epub: usize,
    failed_md: usize,
    failed_docx: usize,
    failed_pdf: usize,
    failed_epub: usize,
    pdf_annotations: usize,
}

struct StatusPanel {
    last_refresh: Instant,
    err: Option<String>,
    raw_line: Option<String>,
    metrics: Option<IndexPassMetrics>,
}

impl Default for LogsPanel {
    fn default() -> Self {
        Self {
            content: String::new(),
            last_refresh: Instant::now() - Duration::from_secs(2),
            err: None,
            last_index_pass: None,
        }
    }
}

impl Default for StatusPanel {
    fn default() -> Self {
        Self {
            last_refresh: Instant::now() - Duration::from_secs(2),
            err: None,
            raw_line: None,
            metrics: None,
        }
    }
}

impl ConfigPanel {
    fn new_standalone(
        _cc: &eframe::CreationContext<'_>,
        shared: Arc<std::sync::Mutex<TrayConfig>>,
    ) -> Self {
        let draft = shared.lock().map(|g| g.clone()).unwrap_or_default();
        Self {
            draft,
            shared,
            err: None,
        }
    }

    fn save_to_disk(&mut self) -> Result<()> {
        let mut g = self.shared.lock().map_err(|_| anyhow::anyhow!("mutex"))?;
        *g = self.draft.clone();
        g.save()?;
        sync_windows_autostart(self.draft.start_with_windows)?;
        Ok(())
    }

    fn request_force_rebuild(&mut self) -> Result<()> {
        self.save_to_disk()?;
        let flag = config::force_rebuild_flag_path();
        if let Some(dir) = flag.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&flag, b"rebuild")?;
        tracing::info!("demande utilisateur : refaire la base maintenant");
        Ok(())
    }
}

fn close_viewport(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
}

impl eframe::App for ConfigPanel {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Indexeur vault (FTS4)");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Dossier du vault :");
                if ui.button("Parcourir…").clicked() {
                    let start: PathBuf = if self.draft.vault_path.is_empty() {
                        dirs::home_dir().unwrap_or_else(|| PathBuf::from("C:\\"))
                    } else {
                        PathBuf::from(&self.draft.vault_path)
                    };
                    if let Some(p) = FileDialog::new().set_directory(start).pick_folder() {
                        self.draft.vault_path = p.to_string_lossy().to_string();
                        self.err = None;
                    }
                }
            });
            ui.text_edit_singleline(&mut self.draft.vault_path);

            ui.add_space(8.0);
            ui.label("Chemin SQLite relatif au vault :");
            ui.text_edit_singleline(&mut self.draft.db_relative);

            ui.horizontal(|ui| {
                ui.label("Intervalle entre deux indexations (s) :");
                ui.add(
                    egui::DragValue::new(&mut self.draft.interval_seconds).range(10_u64..=86_400),
                );
            });

            ui.checkbox(
                &mut self.draft.strip_code_blocks,
                "Ignorer les blocs de code (Markdown)",
            );

            ui.horizontal(|ui| {
                ui.label("Taille max. d’un chunk :");
                ui.add(
                    egui::DragValue::new(&mut self.draft.max_chunk_chars)
                        .range(256_usize..=65536),
                );
            });

            ui.checkbox(
                &mut self.draft.start_enabled,
                "Activer l’indexation au démarrage du tray",
            );
            ui.checkbox(
                &mut self.draft.start_with_windows,
                "Démarrer le tray automatiquement avec Windows",
            );

            if let Some(ref e) = self.err {
                ui.colored_label(egui::Color32::RED, e);
            }

            ui.separator();
            ui.label(
                egui::RichText::new(format!("Fichier : {}", config::config_file_path().display()))
                    .small(),
            );

            ui.horizontal(|ui| {
                if ui.button("Enregistrer").clicked() {
                    match self.save_to_disk() {
                        Ok(()) => close_viewport(ctx),
                        Err(e) => self.err = Some(e.to_string()),
                    }
                }
                if ui.button("Refaire la base maintenant").clicked() {
                    match self.request_force_rebuild() {
                        Ok(()) => {
                            self.err = None;
                            close_viewport(ctx);
                        }
                        Err(e) => self.err = Some(e.to_string()),
                    }
                }
                if ui.button("Ouvrir le JSON (bloc-notes)").clicked() {
                    let p = config::config_file_path();
                    let _ = open_text_editor(&p);
                }
                if ui.button("Fermer").clicked() {
                    close_viewport(ctx);
                }
            });
        });
    }
}

impl LogsPanel {
    fn refresh_if_needed(&mut self) {
        if self.last_refresh.elapsed() < Duration::from_millis(700) {
            return;
        }
        self.last_refresh = Instant::now();
        let path = config::log_file_path();
        match fs::read_to_string(&path) {
            Ok(mut s) => {
                if s.len() > 500_000 {
                    let keep_from = s.len().saturating_sub(500_000);
                    s = s[keep_from..].to_string();
                }
                self.last_index_pass = s
                    .lines()
                    .rev()
                    .find(|l| l.contains("index pass"))
                    .map(|l| l.to_string());
                self.content = s;
                self.err = None;
            }
            Err(e) => {
                self.err = Some(format!("lecture logs: {}", e));
            }
        }
    }
}

fn parse_index_pass(line: &str) -> Option<IndexPassMetrics> {
    if !line.contains("index pass") {
        return None;
    }
    let mut m = IndexPassMetrics::default();
    let mut map = HashMap::<&str, usize>::new();
    for part in line.split_whitespace() {
        if let Some((k, v)) = part.split_once('=') {
            if let Ok(n) = v.trim_matches(',').parse::<usize>() {
                map.insert(k, n);
            }
        }
    }
    m.discovered = *map.get("discovered").unwrap_or(&0);
    m.indexed = *map.get("indexed").unwrap_or(&0);
    m.skipped = *map.get("skipped").unwrap_or(&0);
    m.removed = *map.get("removed").unwrap_or(&0);
    m.indexed_md = *map.get("indexed_md").unwrap_or(&0);
    m.indexed_docx = *map.get("indexed_docx").unwrap_or(&0);
    m.indexed_pdf = *map.get("indexed_pdf").unwrap_or(&0);
    m.indexed_epub = *map.get("indexed_epub").unwrap_or(&0);
    m.skipped_md = *map.get("skipped_md").unwrap_or(&0);
    m.skipped_docx = *map.get("skipped_docx").unwrap_or(&0);
    m.skipped_pdf = *map.get("skipped_pdf").unwrap_or(&0);
    m.skipped_epub = *map.get("skipped_epub").unwrap_or(&0);
    m.failed_md = *map.get("failed_md").unwrap_or(&0);
    m.failed_docx = *map.get("failed_docx").unwrap_or(&0);
    m.failed_pdf = *map.get("failed_pdf").unwrap_or(&0);
    m.failed_epub = *map.get("failed_epub").unwrap_or(&0);
    m.pdf_annotations = *map.get("pdf_annotations").unwrap_or(&0);
    Some(m)
}

impl StatusPanel {
    fn refresh_if_needed(&mut self) {
        if self.last_refresh.elapsed() < Duration::from_millis(700) {
            return;
        }
        self.last_refresh = Instant::now();
        let path = config::log_file_path();
        match fs::read_to_string(&path) {
            Ok(s) => {
                let last = s
                    .lines()
                    .rev()
                    .find(|l| l.contains("index pass"))
                    .map(|l| l.to_string());
                self.metrics = last.as_deref().and_then(parse_index_pass);
                self.raw_line = last;
                self.err = None;
            }
            Err(e) => self.err = Some(format!("lecture logs: {}", e)),
        }
    }
}

impl eframe::App for LogsPanel {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_if_needed();
        ctx.request_repaint_after(Duration::from_millis(500));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Logs indexeur");
                if ui.button("Rafraîchir").clicked() {
                    self.last_refresh = Instant::now() - Duration::from_secs(2);
                    self.refresh_if_needed();
                }
            });
            ui.label(egui::RichText::new(config::log_file_path().display().to_string()).small());
            if let Some(ref e) = self.err {
                ui.colored_label(egui::Color32::RED, e);
            }
            if let Some(ref l) = self.last_index_pass {
                ui.separator();
                ui.label("Dernier bilan d'indexation :");
                let mut last = l.clone();
                ui.add(
                    egui::TextEdit::singleline(&mut last)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY),
                );
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.content)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .desired_rows(35),
                    );
                });
        });
    }
}

impl eframe::App for StatusPanel {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_if_needed();
        ctx.request_repaint_after(Duration::from_millis(700));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Bilan d'indexation");
                if ui.button("Rafraîchir").clicked() {
                    self.last_refresh = Instant::now() - Duration::from_secs(2);
                    self.refresh_if_needed();
                }
            });
            if let Some(ref e) = self.err {
                ui.colored_label(egui::Color32::RED, e);
            }
            if let Some(ref m) = self.metrics {
                ui.separator();
                ui.label(format!(
                    "Fichiers découverts: {} | Indexés: {} | Ignorés inchangés: {} | Retirés: {}",
                    m.discovered, m.indexed, m.skipped, m.removed
                ));
                let done = (m.indexed + m.skipped) as f32;
                let total = m.discovered.max(1) as f32;
                ui.add(
                    egui::ProgressBar::new((done / total).clamp(0.0, 1.0))
                        .text(format!("{:.0}% traités", (done / total) * 100.0)),
                );
                ui.separator();
                egui::Grid::new("bilan_types").striped(true).show(ui, |ui| {
                    ui.label("");
                    ui.label("Markdown");
                    ui.label("DOCX");
                    ui.label("PDF");
                    ui.label("EPUB");
                    ui.end_row();
                    ui.label("Indexés");
                    ui.label(m.indexed_md.to_string());
                    ui.label(m.indexed_docx.to_string());
                    ui.label(m.indexed_pdf.to_string());
                    ui.label(m.indexed_epub.to_string());
                    ui.end_row();
                    ui.label("Ignorés (inchangés)");
                    ui.label(m.skipped_md.to_string());
                    ui.label(m.skipped_docx.to_string());
                    ui.label(m.skipped_pdf.to_string());
                    ui.label(m.skipped_epub.to_string());
                    ui.end_row();
                    ui.label("Erreurs extraction");
                    ui.label(m.failed_md.to_string());
                    ui.label(m.failed_docx.to_string());
                    ui.label(m.failed_pdf.to_string());
                    ui.label(m.failed_epub.to_string());
                    ui.end_row();
                });
                ui.separator();
                ui.label(format!(
                    "Annotations PDF extraites (dernier passage): {}",
                    m.pdf_annotations
                ));
            } else {
                ui.separator();
                ui.label("Aucun bilan `index pass` détecté pour l'instant.");
            }
            if let Some(ref raw) = self.raw_line {
                ui.separator();
                ui.label("Ligne brute (debug) :");
                let mut line = raw.clone();
                ui.add(
                    egui::TextEdit::multiline(&mut line)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(2),
                );
            }
        });
    }
}

fn open_text_editor(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("notepad.exe")
            .arg(path)
            .spawn()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        if std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .is_err()
        {
            std::process::Command::new("open")
                .arg(path)
                .spawn()
                .map_err(|e| anyhow::anyhow!("{}", e))?;
        }
        Ok(())
    }
}

#[cfg(windows)]
fn sync_windows_autostart(enabled: bool) -> Result<()> {
    let exe_path = std::env::current_exe()?;
    let launch_cmd = format!("\"{}\"", exe_path.display());
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run_key, _) =
        hkcu.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")?;
    if enabled {
        run_key.set_value("ObsidianIndexerTray", &launch_cmd)?;
    } else {
        let _ = run_key.delete_value("ObsidianIndexerTray");
    }
    Ok(())
}

#[cfg(not(windows))]
fn sync_windows_autostart(_enabled: bool) -> Result<()> {
    Ok(())
}
