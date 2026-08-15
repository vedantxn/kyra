fn main() {
    const COMMANDS: &[&str] = &[
        "get_dashboard",
        "create_task",
        "set_loop_status",
        "create_calendar_block",
        "hide_overlay",
    ];

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to build the Kyra Tauri application");
}
