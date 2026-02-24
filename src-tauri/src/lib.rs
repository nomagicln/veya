pub mod api_config;
pub mod cast_engine;
pub mod db;
pub mod error;
pub mod learning_record;
pub mod llm_client;
pub mod retry;
pub mod settings;
pub mod stronghold_store;
pub mod text_insight;
pub mod tts_client;
pub mod vision_capture;

use std::sync::Arc;
use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
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

            // --- System Tray ---
            setup_system_tray(app)?;

            // --- Global Shortcut (screenshot capture) ---
            setup_global_shortcut(app)?;

            // --- TextInsightListener (accessibility-based text selection) ---
            let listener = text_insight::TextInsightListener::new(app.handle().clone());
            if let Err(e) = listener.start_listening() {
                log::warn!("Failed to start TextInsightListener: {e}");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api_config::get_api_configs,
            api_config::save_api_config,
            api_config::delete_api_config_cmd,
            api_config::test_api_connection,
            settings::get_settings,
            settings::update_settings,
            text_insight::analyze_text,
            vision_capture::start_capture,
            vision_capture::get_capture_screenshot,
            vision_capture::process_capture,
            cast_engine::generate_podcast,
            cast_engine::save_podcast,
            cast_engine::cleanup_temp_audio,
            cast_engine::cleanup_saved_audio,
            learning_record::save_query_record,
            learning_record::save_podcast_record,
            learning_record::get_query_history,
            learning_record::get_podcast_history,
            learning_record::get_frequent_words,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // --- on_exit hook: cleanup temp audio ---
    app.run(|app_handle, event| {
        if let RunEvent::Exit = event {
            let handle = app_handle.clone();
            // Block on cleanup so temp files are removed before process exits
            tauri::async_runtime::block_on(async move {
                if let Err(e) = cast_engine::cleanup_temp_audio(handle).await {
                    log::warn!("Failed to cleanup temp audio on exit: {e}");
                }
            });
        }
    });
}

/// Configure the system tray with "Open Settings" and "Exit" menu items.
fn setup_system_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;

    let open_settings = MenuItem::with_id(app, "open_settings", "Open Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_settings, &quit])?;

    TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app: &tauri::AppHandle, event| match event.id.as_ref() {
            "open_settings" => {
                // Show or create the settings window
                if let Some(win) = app.get_webview_window("settings") {
                    let _ = win.show();
                    let _ = win.set_focus();
                } else {
                    use tauri::{WebviewUrl, WebviewWindowBuilder};
                    let _ = WebviewWindowBuilder::new(
                        app,
                        "settings",
                        WebviewUrl::App("/settings".into()),
                    )
                    .title("Veya Settings")
                    .inner_size(720.0, 560.0)
                    .build();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}

/// Register the global shortcut for screenshot capture (default: CommandOrControl+Shift+S).
fn setup_global_shortcut(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(desktop)]
    {
        use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

        let capture_shortcut = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyS);

        app.handle().plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut == &capture_shortcut && event.state() == ShortcutState::Pressed {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = vision_capture::start_capture(handle).await {
                                log::warn!("Global shortcut capture failed: {e}");
                            }
                        });
                    }
                })
                .build(),
        )?;

        app.global_shortcut().register(capture_shortcut)?;
    }

    Ok(())
}
