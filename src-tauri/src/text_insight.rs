use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

use crate::api_config::ApiConfig;
use crate::db::Database;
use crate::error::VeyaError;
use crate::llm_client::{LlmClient, LlmConfig, Message};
use crate::retry::RetryPolicy;
use crate::settings::AppSettings;
use crate::stronghold_store::StrongholdStore;

// ── Event types ──────────────────────────────────────────────────

const EVENT_STREAM_CHUNK: &str = "veya://text-insight/stream-chunk";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextInsightChunk {
    #[serde(rename = "type")]
    pub chunk_type: String, // "start" | "delta" | "done" | "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

// ── Language detection ───────────────────────────────────────────

/// Detect the language of the given text using whatlang.
/// Returns a language code string (e.g. "en", "zh", "ja").
pub fn detect_language(text: &str) -> String {
    whatlang::detect_lang(text)
        .map(|lang| whatlang_to_code(lang))
        .unwrap_or_else(|| "unknown".to_string())
}

fn whatlang_to_code(lang: whatlang::Lang) -> String {
    use whatlang::Lang;
    match lang {
        Lang::Eng => "en",
        Lang::Cmn => "zh",
        Lang::Jpn => "ja",
        Lang::Kor => "ko",
        Lang::Fra => "fr",
        Lang::Deu => "de",
        Lang::Spa => "es",
        Lang::Por => "pt",
        Lang::Rus => "ru",
        Lang::Ita => "it",
        _ => lang.code(),
    }
    .to_string()
}

// ── Structured analysis prompt ───────────────────────────────────

fn build_analysis_prompt(text: &str, detected_lang: &str) -> Vec<Message> {
    let system_prompt = r#"You are a language analysis assistant. Analyze the given text and provide a structured response with exactly these six sections, each on its own line prefixed by the section tag:

[ORIGINAL] The original text as-is
[WORD_BY_WORD] Word-by-word or character-by-character explanation with meanings
[STRUCTURE] Grammatical structure analysis (sentence patterns, parts of speech)
[TRANSLATION] Accurate translation to the user's target language
[COLLOQUIAL] A more colloquial/conversational version of the same meaning
[SIMPLIFIED] A simplified version using easier vocabulary

Keep each section concise but informative. Output all six sections in order.
Do not add any extra commentary outside the section tags."#;

    let user_msg = format!(
        "Detected language: {detected_lang}\n\nText to analyze:\n{text}"
    );

    vec![
        Message {
            role: "system".into(),
            content: system_prompt.into(),
        },
        Message {
            role: "user".into(),
            content: user_msg,
        },
    ]
}

// ── Helper: resolve active text model config ─────────────────────

fn resolve_text_llm_config(
    db: &Database,
    store: &StrongholdStore,
    settings: &AppSettings,
) -> Result<(LlmConfig, RetryPolicy), VeyaError> {
    let rows = db.get_api_configs()?;
    let config_row = rows
        .iter()
        .find(|r| r.model_type == "text" && r.is_active)
        .ok_or_else(|| {
            VeyaError::ModelUnavailable(
                "No active text model configured. Please add one in Settings.".into(),
            )
        })?;

    let api_config = ApiConfig::from_row(config_row)?;
    let api_key = if api_config.is_local {
        String::new()
    } else {
        store
            .get_api_key(&api_config.id)?
            .unwrap_or_default()
    };

    let llm_config = LlmConfig {
        provider: api_config.provider,
        base_url: api_config.base_url,
        model_name: api_config.model_name,
        api_key,
    };

    let retry_policy = RetryPolicy::new(settings.retry_count, 500, 10_000);

    Ok((llm_config, retry_policy))
}

// ── Tauri Command ────────────────────────────────────────────────

/// Analyze the given text: detect language, call LLM with structured prompt,
/// and stream results back via Tauri events.
#[tauri::command]
pub async fn analyze_text(
    text: String,
    app: AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    store: tauri::State<'_, Arc<StrongholdStore>>,
) -> Result<(), VeyaError> {
    if text.trim().is_empty() {
        return Err(VeyaError::OcrFailed("Empty text provided".into()));
    }

    let detected_lang = detect_language(&text);

    // Emit start event with detected language
    let _ = app.emit(
        EVENT_STREAM_CHUNK,
        TextInsightChunk {
            chunk_type: "start".into(),
            section: None,
            content: None,
            language: Some(detected_lang.clone()),
        },
    );

    let settings = AppSettings::load(&db)?;
    let (llm_config, retry_policy) = resolve_text_llm_config(&db, &store, &settings)?;

    let messages = build_analysis_prompt(&text, &detected_lang);
    let client = LlmClient::new(llm_config, retry_policy);

    // Use stream_chat which handles start/delta/done/error envelope
    // We use a dedicated event name for text insight
    let result = client
        .stream_chat(messages, &app, EVENT_STREAM_CHUNK)
        .await;

    if let Err(ref e) = result {
        let _ = app.emit(
            EVENT_STREAM_CHUNK,
            TextInsightChunk {
                chunk_type: "error".into(),
                section: None,
                content: Some(e.to_string()),
                language: None,
            },
        );
    }

    result
}

// ── Accessibility Listener ───────────────────────────────────────

/// Platform-agnostic text insight listener that monitors system text selection.
pub struct TextInsightListener {
    app_handle: AppHandle,
}

impl TextInsightListener {
    pub fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }

    /// Start listening for text selection events.
    /// On macOS, uses Accessibility API (AXSelectedTextChanged).
    /// On other platforms, this is a no-op stub for now.
    pub fn start_listening(&self) -> Result<(), VeyaError> {
        #[cfg(target_os = "macos")]
        {
            self.start_macos_listener()?;
        }

        #[cfg(not(target_os = "macos"))]
        {
            log::warn!("Text selection listening is not yet implemented on this platform");
        }

        Ok(())
    }

    /// Called when text is selected by the user in any application.
    /// Triggers the analysis flow.
    pub fn on_text_selected(&self, text: String) {
        if text.trim().is_empty() {
            return;
        }

        let app = self.app_handle.clone();
        tauri::async_runtime::spawn(async move {
            let db = app.state::<Arc<Database>>();
            let store = app.state::<Arc<StrongholdStore>>();

            let detected_lang = detect_language(&text);

            let _ = app.emit(
                EVENT_STREAM_CHUNK,
                TextInsightChunk {
                    chunk_type: "start".into(),
                    section: None,
                    content: None,
                    language: Some(detected_lang.clone()),
                },
            );

            let settings = match AppSettings::load(&db) {
                Ok(s) => s,
                Err(e) => {
                    let _ = app.emit(
                        EVENT_STREAM_CHUNK,
                        TextInsightChunk {
                            chunk_type: "error".into(),
                            section: None,
                            content: Some(e.to_string()),
                            language: None,
                        },
                    );
                    return;
                }
            };

            let (llm_config, retry_policy) = match resolve_text_llm_config(&db, &store, &settings)
            {
                Ok(v) => v,
                Err(e) => {
                    let _ = app.emit(
                        EVENT_STREAM_CHUNK,
                        TextInsightChunk {
                            chunk_type: "error".into(),
                            section: None,
                            content: Some(e.to_string()),
                            language: None,
                        },
                    );
                    return;
                }
            };

            let messages = build_analysis_prompt(&text, &detected_lang);
            let client = LlmClient::new(llm_config, retry_policy);

            if let Err(e) = client
                .stream_chat(messages, &app, EVENT_STREAM_CHUNK)
                .await
            {
                let _ = app.emit(
                    EVENT_STREAM_CHUNK,
                    TextInsightChunk {
                        chunk_type: "error".into(),
                        section: None,
                        content: Some(e.to_string()),
                        language: None,
                    },
                );
            }
        });
    }
}

// ── macOS Accessibility API implementation ───────────────────────

#[cfg(target_os = "macos")]
mod macos_a11y {
    use super::*;
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};
    use std::ffi::c_void;
    use std::os::raw::c_int;

    // Wrapper to allow sending a raw pointer across threads.
    // SAFETY: The pointer is only dereferenced on the thread that owns it,
    // and the pointed-to TextInsightListener is heap-allocated and lives
    // for the entire application lifetime.
    struct SendPtr(usize);
    impl SendPtr {
        fn from_ptr(p: *mut c_void) -> Self {
            Self(p as usize)
        }
        fn as_ptr(&self) -> *mut c_void {
            self.0 as *mut c_void
        }
    }

    // AXObserver C API bindings
    type AXObserverRef = *mut c_void;
    type AXUIElementRef = *mut c_void;
    type CFRunLoopSourceRef = *mut c_void;
    type CFRunLoopRef = *mut c_void;

    type AXObserverCallback = extern "C" fn(
        observer: AXObserverRef,
        element: AXUIElementRef,
        notification: CFStringRef,
        refcon: *mut c_void,
    );

    extern "C" {
        fn AXUIElementCreateSystemWide() -> AXUIElementRef;
        fn AXObserverCreate(
            application: i32,
            callback: AXObserverCallback,
            observer: *mut AXObserverRef,
        ) -> c_int;
        fn AXObserverAddNotification(
            observer: AXObserverRef,
            element: AXUIElementRef,
            notification: CFStringRef,
            refcon: *mut c_void,
        ) -> c_int;
        fn AXObserverGetRunLoopSource(observer: AXObserverRef) -> CFRunLoopSourceRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut *mut c_void,
        ) -> c_int;
        fn AXIsProcessTrusted() -> bool;
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
        fn CFRunLoopRun();
    }

    // kAXSelectedTextChangedNotification
    const AX_SELECTED_TEXT_CHANGED: &str = "AXSelectedTextChanged";
    const AX_SELECTED_TEXT_ATTRIBUTE: &str = "AXSelectedText";
    const K_CF_RUN_LOOP_DEFAULT_MODE: &str = "kCFRunLoopDefaultMode";

    /// Check if the process has Accessibility permissions on macOS.
    pub fn is_accessibility_trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    /// Callback invoked by the AXObserver when text selection changes.
    extern "C" fn ax_notification_callback(
        _observer: AXObserverRef,
        element: AXUIElementRef,
        _notification: CFStringRef,
        refcon: *mut c_void,
    ) {
        unsafe {
            let attr_name = CFString::new(AX_SELECTED_TEXT_ATTRIBUTE);
            let mut value: *mut c_void = std::ptr::null_mut();

            let result = AXUIElementCopyAttributeValue(
                element,
                attr_name.as_concrete_TypeRef(),
                &mut value,
            );

            if result == 0 && !value.is_null() {
                // value is a CFStringRef
                let cf_str = CFString::wrap_under_get_rule(value as CFStringRef);
                let text = cf_str.to_string();

                if !text.trim().is_empty() {
                    // refcon is a raw pointer to our AppHandle clone
                    let listener = &*(refcon as *const TextInsightListener);
                    listener.on_text_selected(text);
                }
            }
        }
    }

    impl TextInsightListener {
        /// Start the macOS Accessibility observer on a background thread.
        pub(super) fn start_macos_listener(&self) -> Result<(), VeyaError> {
            if !is_accessibility_trusted() {
                return Err(VeyaError::PermissionDenied(
                    "Accessibility permission not granted. Please enable it in System Settings > Privacy & Security > Accessibility.".into(),
                ));
            }

            // Leak a clone of self to keep it alive for the callback.
            // This is intentional — the listener lives for the app's lifetime.
            let listener = Box::new(TextInsightListener {
                app_handle: self.app_handle.clone(),
            });
            let send_refcon = SendPtr::from_ptr(Box::into_raw(listener) as *mut c_void);

            std::thread::spawn(move || {
                let refcon = send_refcon.as_ptr();
                unsafe {
                    let system_wide = AXUIElementCreateSystemWide();

                    let mut observer: AXObserverRef = std::ptr::null_mut();
                    // pid 0 = system-wide observer
                    let err = AXObserverCreate(0, ax_notification_callback, &mut observer);
                    if err != 0 {
                        log::error!("Failed to create AXObserver: error code {err}");
                        return;
                    }

                    let notification = CFString::new(AX_SELECTED_TEXT_CHANGED);
                    let err = AXObserverAddNotification(
                        observer,
                        system_wide,
                        notification.as_concrete_TypeRef(),
                        refcon,
                    );
                    if err != 0 {
                        log::error!(
                            "Failed to add AXSelectedTextChanged notification: error code {err}"
                        );
                        return;
                    }

                    let source = AXObserverGetRunLoopSource(observer);
                    let run_loop = CFRunLoopGetCurrent();
                    let mode = CFString::new(K_CF_RUN_LOOP_DEFAULT_MODE);
                    CFRunLoopAddSource(run_loop, source, mode.as_concrete_TypeRef());

                    log::info!("macOS Accessibility listener started");
                    CFRunLoopRun();
                }
            });

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_language_english() {
        let lang = detect_language(
            "The quick brown fox jumps over the lazy dog. This is a simple English sentence for testing purposes.",
        );
        assert_eq!(lang, "en");
    }

    #[test]
    fn detect_language_chinese() {
        let lang = detect_language("你好，今天天气怎么样？");
        assert_eq!(lang, "zh");
    }

    #[test]
    fn detect_language_empty_returns_unknown() {
        let lang = detect_language("");
        assert_eq!(lang, "unknown");
    }

    #[test]
    fn detect_language_short_text() {
        // Very short text may not be reliably detected
        let lang = detect_language("hi");
        // Should return something, not panic
        assert!(!lang.is_empty());
    }

    #[test]
    fn build_prompt_contains_text() {
        let messages = build_analysis_prompt("Hello world", "en");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "system");
        assert!(messages[1].content.contains("Hello world"));
        assert!(messages[1].content.contains("en"));
    }
}
