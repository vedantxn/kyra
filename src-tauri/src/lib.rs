mod commands;
mod db;
mod types;

use sqlx::SqlitePool;
use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub struct AppState {
    pool: SqlitePool,
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
            app.manage(AppState { pool });
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kyra");
}
