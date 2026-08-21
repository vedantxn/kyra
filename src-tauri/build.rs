fn main() {
    println!("cargo:rerun-if-changed=../.env.local");
    if let Ok(contents) = std::fs::read_to_string("../.env.local") {
        for env_key in ["KYRA_GOOGLE_CLIENT_ID", "KYRA_GOOGLE_CLIENT_SECRET"] {
            if let Some(value) = contents.lines().find_map(|line| {
                let line = line.trim();
                if line.starts_with('#') {
                    return None;
                }
                let (key, value) = line.split_once('=')?;
                (key.trim() == env_key)
                    .then(|| value.trim().trim_matches(['\'', '"']).to_owned())
                    .filter(|value| !value.is_empty())
            }) {
                println!("cargo:rustc-env={env_key}={value}");
            }
        }
    }

    const COMMANDS: &[&str] = &[
        "get_dashboard",
        "create_task",
        "set_loop_status",
        "create_calendar_block",
        "hide_overlay",
        "get_google_connector_status",
        "connect_google",
        "disconnect_google",
        "sync_google_now",
        "mutate_google_calendar",
    ];

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to build the Kyra Tauri application");
}
