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
            settings::update_capture_shortcut,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // --- Run loop: hide-on-close + cleanup on exit ---
    app.run(|app_handle, event| {
        match event {
            // When a window close is requested, hide it instead of destroying it
            RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } => {
                // Prevent the window from actually closing
                api.prevent_close();
                // Just hide it so the app stays in the tray
                if let Some(win) = app_handle.get_webview_window(&label) {
                    let _ = win.hide();
                }
            }
            RunEvent::Exit => {
                let handle = app_handle.clone();
                // Block on cleanup so temp files are removed before process exits
                tauri::async_runtime::block_on(async move {
                    if let Err(e) = cast_engine::cleanup_temp_audio(handle).await {
                        log::warn!("Failed to cleanup temp audio on exit: {e}");
                    }
                });
            }
            _ => {}
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

    const TRAY_ICON: tauri::image::Image<'_> = tauri::include_image!("icons/32x32.png");

    TrayIconBuilder::new()
        .icon(TRAY_ICON)
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app: &tauri::AppHandle, event| match event.id.as_ref() {
            "open_settings" => {
                // Show the main window (which contains the settings page)
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
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

/// Parse a shortcut string like "CommandOrControl+Shift+S" into a Tauri Shortcut.
#[cfg(desktop)]
pub fn parse_shortcut(s: &str) -> Option<tauri_plugin_global_shortcut::Shortcut> {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

    let mut modifiers = Modifiers::empty();
    let mut code: Option<Code> = None;

    for part in s.split('+') {
        let part = part.trim();
        match part {
            "CommandOrControl" | "CmdOrCtrl" => modifiers |= Modifiers::SUPER,
            "Shift" => modifiers |= Modifiers::SHIFT,
            "Alt" | "Option" => modifiers |= Modifiers::ALT,
            "Control" | "Ctrl" => modifiers |= Modifiers::CONTROL,
            "Super" | "Meta" | "Command" | "Cmd" => modifiers |= Modifiers::SUPER,
            other => {
                code = match other.to_uppercase().as_str() {
                    "A" => Some(Code::KeyA), "B" => Some(Code::KeyB), "C" => Some(Code::KeyC),
                    "D" => Some(Code::KeyD), "E" => Some(Code::KeyE), "F" => Some(Code::KeyF),
                    "G" => Some(Code::KeyG), "H" => Some(Code::KeyH), "I" => Some(Code::KeyI),
                    "J" => Some(Code::KeyJ), "K" => Some(Code::KeyK), "L" => Some(Code::KeyL),
                    "M" => Some(Code::KeyM), "N" => Some(Code::KeyN), "O" => Some(Code::KeyO),
                    "P" => Some(Code::KeyP), "Q" => Some(Code::KeyQ), "R" => Some(Code::KeyR),
                    "S" => Some(Code::KeyS), "T" => Some(Code::KeyT), "U" => Some(Code::KeyU),
                    "V" => Some(Code::KeyV), "W" => Some(Code::KeyW), "X" => Some(Code::KeyX),
                    "Y" => Some(Code::KeyY), "Z" => Some(Code::KeyZ),
                    "0" => Some(Code::Digit0), "1" => Some(Code::Digit1), "2" => Some(Code::Digit2),
                    "3" => Some(Code::Digit3), "4" => Some(Code::Digit4), "5" => Some(Code::Digit5),
                    "6" => Some(Code::Digit6), "7" => Some(Code::Digit7), "8" => Some(Code::Digit8),
                    "9" => Some(Code::Digit9),
                    "F1" => Some(Code::F1), "F2" => Some(Code::F2), "F3" => Some(Code::F3),
                    "F4" => Some(Code::F4), "F5" => Some(Code::F5), "F6" => Some(Code::F6),
                    "F7" => Some(Code::F7), "F8" => Some(Code::F8), "F9" => Some(Code::F9),
                    "F10" => Some(Code::F10), "F11" => Some(Code::F11), "F12" => Some(Code::F12),
                    "SPACE" => Some(Code::Space), "ENTER" => Some(Code::Enter),
                    "ESCAPE" | "ESC" => Some(Code::Escape),
                    "UP" => Some(Code::ArrowUp), "DOWN" => Some(Code::ArrowDown),
                    "LEFT" => Some(Code::ArrowLeft), "RIGHT" => Some(Code::ArrowRight),
                    "BACKSPACE" => Some(Code::Backspace), "DELETE" => Some(Code::Delete),
                    "TAB" => Some(Code::Tab), "HOME" => Some(Code::Home), "END" => Some(Code::End),
                    "PAGEUP" => Some(Code::PageUp), "PAGEDOWN" => Some(Code::PageDown),
                    "[" => Some(Code::BracketLeft), "]" => Some(Code::BracketRight),
                    "\\" => Some(Code::Backslash), ";" => Some(Code::Semicolon),
                    "'" => Some(Code::Quote), "," => Some(Code::Comma), "." => Some(Code::Period),
                    "/" => Some(Code::Slash), "-" => Some(Code::Minus), "=" => Some(Code::Equal),
                    "`" => Some(Code::Backquote),
                    _ => None,
                };
            }
        }
    }

    let mods = if modifiers.is_empty() { None } else { Some(modifiers) };
    code.map(|c| Shortcut::new(mods, c))
}

/// Register the global shortcut for screenshot capture, reading from settings.
fn setup_global_shortcut(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(desktop)]
    {
        use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

        // Read shortcut from DB, fall back to default
        let db = app.state::<Arc<db::Database>>();
        let app_settings = settings::AppSettings::load(&db).unwrap_or_default();
        let shortcut_str = app_settings.shortcut_capture;

        app.handle().plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
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

        if let Some(shortcut) = parse_shortcut(&shortcut_str) {
            if let Err(e) = app.global_shortcut().register(shortcut) {
                log::warn!("Failed to register shortcut '{shortcut_str}': {e}");
            }
        } else {
            log::warn!("Failed to parse shortcut string: {shortcut_str}");
        }
    }

    Ok(())
}

