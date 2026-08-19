pub mod ai;
mod commands;
mod crypto;
mod db;
mod google;
mod types;

use sqlx::SqlitePool;
use std::sync::Arc;
#[cfg(target_os = "macos")]
use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub struct AppState {
    pool: SqlitePool,
    cipher: crypto::LocalCipher,
    google: Arc<google::GoogleConnector>,
    ai: Arc<ai::runtime::AiEngine>,
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
            let (pool, cipher) = tauri::async_runtime::block_on(db::initialize(app.handle()))?;
            let google = google::GoogleConnector::new(pool.clone());
            let ai = ai::runtime::AiEngine::new(
                pool.clone(),
                cipher.clone(),
                google.clone(),
                Some(app.handle().clone()),
            );
            let scheduled_connector = google.clone();
            app.manage(AppState {
                pool,
                cipher,
                google,
                ai: ai.clone(),
            });
            ai.start_scheduler();
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
                #[cfg(target_os = "macos")]
                window.set_effects(
                    EffectsBuilder::new()
                        .effect(Effect::UnderWindowBackground)
                        .state(EffectState::Active)
                        .build(),
                )?;
                window.maximize()?;
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
            commands::get_ai_engine_status,
            commands::save_ai_provider_config,
            commands::clear_ai_provider,
            commands::list_ollama_models,
            commands::test_ai_provider,
            commands::run_ai_now,
            commands::execute_ai_command,
            commands::list_ai_reviews,
            commands::resolve_ai_review,
            commands::list_ai_activity,
            commands::retry_ai_job,
            commands::revert_ai_action,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Kyra")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen {
                has_visible_windows: false,
                ..
            } = event
            {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.maximize();
                    let _ = window.set_focus();
                }
            }
        });
}
