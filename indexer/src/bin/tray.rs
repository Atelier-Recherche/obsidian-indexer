#![cfg_attr(windows, windows_subsystem = "windows")]

//! Lance l’indexeur avec icône dans la zone de notification.
//!
//! Fenêtre de configuration (processus séparé, évite deux EventLoop / winit sur Windows) :
//! même exécutable avec `--config-gui`

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config_gui = args.iter().any(|a| a == "--config-gui");
    let status_gui = args.iter().any(|a| a == "--status-gui");
    let logs_gui = args.iter().any(|a| a == "--logs-gui");
    if config_gui {
        return obsidian_indexer::tray_daemon::run_config_gui_standalone();
    }
    if status_gui {
        return obsidian_indexer::tray_daemon::run_status_gui_standalone();
    }
    if logs_gui {
        return obsidian_indexer::tray_daemon::run_logs_gui_standalone();
    }
    obsidian_indexer::tray_daemon::run()
}
