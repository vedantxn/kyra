mod commands;
mod db;
mod google;
mod types;

use sqlx::SqlitePool;
use std::sync::Arc;
use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub struct AppState {
    pool: SqlitePool,
    google: Arc<google::GoogleConnector>,
}

pub fn run() {
    let command_k = Shortcut::new(Some(Modifiers::SUPER), Code::KeyK);
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if !shortcut.matches(Modifiers::SUPER, Code::KeyK)
                        || event.state() != ShortcutState::Pressed
                    {
                        return;
                    }
                    if let Some(window) = app.get_webview_window("main") {
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(),
        )
        .setup(move |app| {
            let pool = tauri::async_runtime::block_on(db::initialize(app.handle()))?;
            let google = google::GoogleConnector::new(pool.clone());
            let scheduled_connector = google.clone();
            app.manage(AppState { pool, google });
            tauri::async_runtime::spawn(async move {
                let _ = scheduled_connector.sync_now().await;
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
                interval.tick().await;
                loop {
                    interval.tick().await;
                    scheduled_connector.sync_if_due().await;
                }
            });
            app.global_shortcut().register(command_k)?;
            if let Some(window) = app.get_webview_window("main") {
                window.show()?;
                window.set_focus()?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_dashboard,
            commands::create_task,
            commands::set_loop_status,
            commands::create_calendar_block,
            commands::hide_overlay,
            commands::get_google_connector_status,
            commands::connect_google,
            commands::disconnect_google,
            commands::sync_google_now,
            commands::mutate_google_calendar,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kyra");
}
