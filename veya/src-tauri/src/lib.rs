pub mod api_config;
pub mod db;
pub mod error;
pub mod llm_client;
pub mod retry;
pub mod settings;
pub mod stronghold_store;
pub mod tts_client;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_stronghold::Builder::new(|password| {
            use std::hash::{DefaultHasher, Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            password.hash(&mut hasher);
            hasher.finish().to_le_bytes().to_vec()
        }).build())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().expect("failed to resolve app data dir");

            let database = Arc::new(
                db::Database::open(app_data_dir.clone())
                    .expect("failed to open database"),
            );

            let stronghold = Arc::new(
                stronghold_store::StrongholdStore::open(app_data_dir, b"veya-default-pw")
                    .expect("failed to open stronghold"),
            );

            app.manage(database);
            app.manage(stronghold);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api_config::get_api_configs,
            api_config::save_api_config,
            api_config::delete_api_config_cmd,
            api_config::test_api_connection,
            settings::get_settings,
            settings::update_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
