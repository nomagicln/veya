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

// ── Constants ────────────────────────────────────────────────────

const EVENT_STREAM_CHUNK: &str = "veya://vision-capture/stream-chunk";

// ── Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureRegion {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionCaptureChunk {
    #[serde(rename = "type")]
    pub chunk_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_ai_inferred: Option<bool>,
}

// ── Platform-specific screenshot capture ─────────────────────────

pub fn capture_screen() -> Result<Vec<u8>, VeyaError> {
    #[cfg(target_os = "macos")]
    {
        macos_capture::capture_full_screen()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(VeyaError::OcrFailed("Screen capture not supported on this platform".into()))
    }
}

pub fn crop_image(image_data: &[u8], region: &CaptureRegion) -> Result<Vec<u8>, VeyaError> {
    #[cfg(target_os = "macos")]
    {
        macos_capture::crop_png(image_data, region)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (image_data, region);
        Err(VeyaError::OcrFailed("Image cropping not supported on this platform".into()))
    }
}

// ── Platform-specific OCR ────────────────────────────────────────

pub fn recognize_text(image_data: &[u8]) -> Result<String, VeyaError> {
    #[cfg(target_os = "macos")]
    {
        macos_ocr::recognize(image_data)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = image_data;
        Err(VeyaError::OcrFailed("OCR not supported on this platform".into()))
    }
}

// ── AI completion prompt ─────────────────────────────────────────

fn build_ocr_completion_prompt(ocr_text: &str) -> Vec<Message> {
    let system_prompt = r#"You are an OCR post-processing assistant. The user will provide text recognized by OCR from a screenshot. Your job is to:

1. Fix any obvious OCR errors (misrecognized characters, broken words)
2. Infer and complete any truncated or partially visible text
3. Preserve the original structure and formatting

Output your response in this exact format:
[CORRECTED] The corrected/completed full text
[INFERRED] A comma-separated list of phrases or words that you inferred or corrected (that were NOT in the original OCR output). If nothing was inferred, write "none".

Be conservative — only infer content when you have high confidence."#;

    vec![
        Message { role: "system".into(), content: system_prompt.into() },
        Message { role: "user".into(), content: format!("OCR recognized text:\n{ocr_text}") },
    ]
}

/// Parse the AI completion response to extract corrected text and inferred parts.
pub fn parse_completion_response(response: &str) -> (String, Vec<String>) {
    let mut corrected = String::new();
    let mut inferred = Vec::new();
    let mut in_corrected = false;

    for line in response.lines() {
        if let Some(text) = line.strip_prefix("[CORRECTED]") {
            corrected = text.trim().to_string();
            in_corrected = true;
        } else if let Some(text) = line.strip_prefix("[INFERRED]") {
            in_corrected = false;
            let trimmed = text.trim();
            if trimmed != "none" && !trimmed.is_empty() {
                inferred = trimmed.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            }
        } else if in_corrected {
            if !corrected.is_empty() {
                corrected.push('\n');
            }
            corrected.push_str(line);
        }
    }

    if corrected.is_empty() {
        corrected = response.to_string();
    }

    (corrected, inferred)
}

// ── Helper: resolve active vision/text model config ──────────────

fn resolve_vision_llm_config(
    db: &Database,
    store: &StrongholdStore,
    settings: &AppSettings,
) -> Result<(LlmConfig, RetryPolicy), VeyaError> {
    let rows = db.get_api_configs()?;
    let config_row = rows
        .iter()
        .find(|r| r.model_type == "vision" && r.is_active)
        .or_else(|| rows.iter().find(|r| r.model_type == "text" && r.is_active))
        .ok_or_else(|| {
            VeyaError::ModelUnavailable(
                "No active vision or text model configured. Please add one in Settings.".into(),
            )
        })?;

    let api_config = ApiConfig::from_row(config_row)?;
    let api_key = if api_config.is_local {
        String::new()
    } else {
        store.get_api_key(&api_config.id)?.unwrap_or_default()
    };

    Ok((
        LlmConfig {
            provider: api_config.provider,
            base_url: api_config.base_url,
            model_name: api_config.model_name,
            api_key,
        },
        RetryPolicy::new(settings.retry_count, 500, 10_000),
    ))
}

// ── Tauri Commands ───────────────────────────────────────────────

/// Shared state holding the latest full-screen screenshot bytes.
pub struct CaptureScreenshot(pub Arc<Vec<u8>>);

/// Start the capture flow: screenshot the screen, then open the overlay window.
#[tauri::command]
pub async fn start_capture(app: AppHandle) -> Result<(), VeyaError> {
    let screenshot_bytes = capture_screen()?;
    app.manage(CaptureScreenshot(Arc::new(screenshot_bytes)));

    if let Some(overlay) = app.get_webview_window("capture-overlay") {
        let _ = overlay.show();
        let _ = overlay.set_focus();
    } else {
        use tauri::{WebviewUrl, WebviewWindowBuilder};
        let _overlay = WebviewWindowBuilder::new(
            &app,
            "capture-overlay",
            WebviewUrl::App("/capture".into()),
        )
        .title("Veya Capture")
        .fullscreen(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .build()
        .map_err(|e| VeyaError::OcrFailed(format!("Failed to create capture overlay: {e}")))?;
    }

    Ok(())
}

/// Get the current screenshot as base64 for the overlay to display.
#[tauri::command]
pub async fn get_capture_screenshot(
    screenshot: tauri::State<'_, CaptureScreenshot>,
) -> Result<String, VeyaError> {
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(screenshot.0.as_ref()))
}

/// Process a captured region: crop, OCR, optionally AI-complete, then stream results.
#[tauri::command]
pub async fn process_capture(
    region: CaptureRegion,
    ai_completion: bool,
    app: AppHandle,
    db: tauri::State<'_, Arc<Database>>,
    store: tauri::State<'_, Arc<StrongholdStore>>,
) -> Result<(), VeyaError> {
    let screenshot = app
        .try_state::<CaptureScreenshot>()
        .ok_or_else(|| VeyaError::OcrFailed("No screenshot available. Call start_capture first.".into()))?;
    let image_data = screenshot.0.as_ref().clone();

    // Close the capture overlay
    if let Some(overlay) = app.get_webview_window("capture-overlay") {
        let _ = overlay.close();
    }

    // Crop to the selected region
    let cropped = crop_image(&image_data, &region)?;

    // Run native OCR
    let ocr_text = recognize_text(&cropped)?;
    if ocr_text.trim().is_empty() {
        return Err(VeyaError::OcrFailed("No text recognized in the selected region".into()));
    }

    // Emit OCR result
    let _ = app.emit(EVENT_STREAM_CHUNK, VisionCaptureChunk {
        chunk_type: "ocr_result".into(),
        content: Some(ocr_text.clone()),
        is_ai_inferred: Some(false),
    });

    // Optionally run AI completion
    if ai_completion {
        let settings = AppSettings::load(&db)?;
        let (llm_config, retry_policy) = resolve_vision_llm_config(&db, &store, &settings)?;
        let client = LlmClient::new(llm_config, retry_policy);

        match client.chat(build_ocr_completion_prompt(&ocr_text)).await {
            Ok(response) => {
                let (corrected, inferred_parts) = parse_completion_response(&response);
                let _ = app.emit(EVENT_STREAM_CHUNK, VisionCaptureChunk {
                    chunk_type: "ai_completion".into(),
                    content: Some(corrected),
                    is_ai_inferred: Some(true),
                });
                if !inferred_parts.is_empty() {
                    let _ = app.emit(EVENT_STREAM_CHUNK, VisionCaptureChunk {
                        chunk_type: "analysis_delta".into(),
                        content: Some(serde_json::to_string(&inferred_parts).unwrap_or_default()),
                        is_ai_inferred: Some(true),
                    });
                }
            }
            Err(e) => {
                let _ = app.emit(EVENT_STREAM_CHUNK, VisionCaptureChunk {
                    chunk_type: "error".into(),
                    content: Some(format!("AI completion failed: {e}")),
                    is_ai_inferred: None,
                });
            }
        }
    }

    // Emit done
    let _ = app.emit(EVENT_STREAM_CHUNK, VisionCaptureChunk {
        chunk_type: "done".into(),
        content: None,
        is_ai_inferred: None,
    });

    Ok(())
}

// ── macOS: Screenshot via Core Graphics ──────────────────────────

#[cfg(target_os = "macos")]
mod macos_capture {
    use super::*;
    use std::ffi::c_void;

    // CGImage / ImageIO FFI — these are C APIs, not Objective-C, so direct extern is fine.
    type CGImageRef = *mut c_void;
    type CFDataRef = *const c_void;
    type CFMutableDataRef = *mut c_void;
    type CGImageSourceRef = *mut c_void;
    type CGImageDestinationRef = *mut c_void;
    type CFStringRef = *const c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint { x: f64, y: f64 }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize { width: f64, height: f64 }

    extern "C" {
        fn CGWindowListCreateImage(bounds: CGRect, opts: u32, wid: u32, img_opt: u32) -> CGImageRef;
        fn CGImageCreateWithImageInRect(image: CGImageRef, rect: CGRect) -> CGImageRef;
        fn CGImageRelease(image: CGImageRef);
        fn CGMainDisplayID() -> u32;
        fn CGDisplayPixelsWide(display: u32) -> usize;
        fn CGDisplayPixelsHigh(display: u32) -> usize;

        // ImageIO
        fn CGImageDestinationCreateWithData(data: CFMutableDataRef, ty: CFStringRef, count: usize, opts: *const c_void) -> CGImageDestinationRef;
        fn CGImageDestinationAddImage(dest: CGImageDestinationRef, image: CGImageRef, props: *const c_void);
        fn CGImageDestinationFinalize(dest: CGImageDestinationRef) -> bool;

        fn CGImageSourceCreateWithData(data: CFDataRef, opts: *const c_void) -> CGImageSourceRef;
        fn CGImageSourceCreateImageAtIndex(src: CGImageSourceRef, idx: usize, opts: *const c_void) -> CGImageRef;

        // CoreFoundation
        fn CFDataCreateMutable(alloc: *const c_void, cap: isize) -> CFMutableDataRef;
        fn CFDataCreate(alloc: *const c_void, bytes: *const u8, len: isize) -> CFDataRef;
        fn CFDataGetLength(data: CFDataRef) -> isize;
        fn CFDataGetBytePtr(data: CFDataRef) -> *const u8;
        fn CFRelease(cf: *const c_void);

        static kUTTypePNG: CFStringRef;
    }

    pub fn capture_full_screen() -> Result<Vec<u8>, VeyaError> {
        unsafe {
            let display = CGMainDisplayID();
            let w = CGDisplayPixelsWide(display) as f64;
            let h = CGDisplayPixelsHigh(display) as f64;

            let rect = CGRect { origin: CGPoint { x: 0.0, y: 0.0 }, size: CGSize { width: w, height: h } };
            let image = CGWindowListCreateImage(rect, 1 /* onScreenOnly */, 0, 0);
            if image.is_null() {
                return Err(VeyaError::OcrFailed("CGWindowListCreateImage returned null".into()));
            }
            let result = cgimage_to_png(image);
            CGImageRelease(image);
            result
        }
    }

    pub fn crop_png(png_data: &[u8], region: &CaptureRegion) -> Result<Vec<u8>, VeyaError> {
        unsafe {
            let cg_image = cgimage_from_png(png_data)?;
            let rect = CGRect {
                origin: CGPoint { x: region.x, y: region.y },
                size: CGSize { width: region.width, height: region.height },
            };
            let cropped = CGImageCreateWithImageInRect(cg_image, rect);
            if cropped.is_null() {
                CGImageRelease(cg_image);
                return Err(VeyaError::OcrFailed("Failed to crop image".into()));
            }
            let result = cgimage_to_png(cropped);
            CGImageRelease(cropped);
            CGImageRelease(cg_image);
            result
        }
    }

    unsafe fn cgimage_to_png(image: CGImageRef) -> Result<Vec<u8>, VeyaError> {
        let md = CFDataCreateMutable(std::ptr::null(), 0);
        if md.is_null() { return Err(VeyaError::OcrFailed("CFDataCreateMutable failed".into())); }

        let dest = CGImageDestinationCreateWithData(md, kUTTypePNG, 1, std::ptr::null());
        if dest.is_null() { CFRelease(md as _); return Err(VeyaError::OcrFailed("CGImageDestinationCreate failed".into())); }

        CGImageDestinationAddImage(dest, image, std::ptr::null());
        if !CGImageDestinationFinalize(dest) {
            CFRelease(dest as _); CFRelease(md as _);
            return Err(VeyaError::OcrFailed("CGImageDestinationFinalize failed".into()));
        }

        let len = CFDataGetLength(md as CFDataRef) as usize;
        let ptr = CFDataGetBytePtr(md as CFDataRef);
        let bytes = std::slice::from_raw_parts(ptr, len).to_vec();
        CFRelease(dest as _);
        CFRelease(md as _);
        Ok(bytes)
    }

    unsafe fn cgimage_from_png(data: &[u8]) -> Result<CGImageRef, VeyaError> {
        let cf_data = CFDataCreate(std::ptr::null(), data.as_ptr(), data.len() as isize);
        if cf_data.is_null() { return Err(VeyaError::OcrFailed("CFDataCreate failed".into())); }

        let src = CGImageSourceCreateWithData(cf_data, std::ptr::null());
        if src.is_null() { CFRelease(cf_data); return Err(VeyaError::OcrFailed("CGImageSourceCreate failed".into())); }

        let image = CGImageSourceCreateImageAtIndex(src, 0, std::ptr::null());
        CFRelease(src as _);
        CFRelease(cf_data);
        if image.is_null() { return Err(VeyaError::OcrFailed("Failed to decode PNG".into())); }
        Ok(image)
    }
}

// ── macOS: OCR via Vision Framework (using objc crate) ───────────

#[cfg(target_os = "macos")]
mod macos_ocr {
    use super::*;
    use objc::runtime::{Class, Object, BOOL, YES};
    use objc::{msg_send, sel, sel_impl};
    use std::ffi::c_void;

    /// Perform OCR on PNG image bytes using macOS Vision Framework.
    pub fn recognize(image_data: &[u8]) -> Result<String, VeyaError> {
        unsafe { recognize_inner(image_data) }
    }

    unsafe fn recognize_inner(image_data: &[u8]) -> Result<String, VeyaError> {
        // 1. Create NSData from bytes
        let nsdata_cls = Class::get("NSData")
            .ok_or_else(|| VeyaError::OcrFailed("NSData class not found".into()))?;
        let nsdata: *mut Object = msg_send![nsdata_cls,
            dataWithBytes: image_data.as_ptr() as *const c_void
            length: image_data.len()
        ];
        if nsdata.is_null() {
            return Err(VeyaError::OcrFailed("Failed to create NSData".into()));
        }

        // 2. Create VNImageRequestHandler
        let handler_cls = Class::get("VNImageRequestHandler")
            .ok_or_else(|| VeyaError::OcrFailed("VNImageRequestHandler class not found".into()))?;
        let dict_cls = Class::get("NSDictionary")
            .ok_or_else(|| VeyaError::OcrFailed("NSDictionary class not found".into()))?;
        let empty_dict: *mut Object = msg_send![dict_cls, dictionary];

        let handler: *mut Object = msg_send![handler_cls, alloc];
        let handler: *mut Object = msg_send![handler,
            initWithData: nsdata
            options: empty_dict
        ];
        if handler.is_null() {
            return Err(VeyaError::OcrFailed("Failed to create VNImageRequestHandler".into()));
        }

        // 3. Create VNRecognizeTextRequest
        let request_cls = Class::get("VNRecognizeTextRequest")
            .ok_or_else(|| VeyaError::OcrFailed("VNRecognizeTextRequest class not found".into()))?;
        let request: *mut Object = msg_send![request_cls, alloc];
        let request: *mut Object = msg_send![request, init];
        if request.is_null() {
            return Err(VeyaError::OcrFailed("Failed to create VNRecognizeTextRequest".into()));
        }

        // Set recognition level to accurate (1)
        let _: () = msg_send![request, setRecognitionLevel: 1i64];
        // Enable automatic language detection
        let _: () = msg_send![request, setAutomaticallyDetectsLanguage: YES];

        // 4. Wrap request in NSArray
        let array_cls = Class::get("NSArray")
            .ok_or_else(|| VeyaError::OcrFailed("NSArray class not found".into()))?;
        let requests_array: *mut Object = msg_send![array_cls, arrayWithObject: request];

        // 5. Perform the request
        let mut error: *mut Object = std::ptr::null_mut();
        let success: BOOL = msg_send![handler,
            performRequests: requests_array
            error: &mut error as *mut *mut Object
        ];

        if success == objc::runtime::NO {
            let desc = if !error.is_null() {
                let ns: *mut Object = msg_send![error, localizedDescription];
                nsstring_to_rust(ns)
            } else {
                "Unknown error".to_string()
            };
            return Err(VeyaError::OcrFailed(format!("Vision OCR failed: {desc}")));
        }

        // 6. Extract results
        let results: *mut Object = msg_send![request, results];
        if results.is_null() {
            return Ok(String::new());
        }

        let count: usize = msg_send![results, count];
        let mut text_parts = Vec::new();

        for i in 0..count {
            let observation: *mut Object = msg_send![results, objectAtIndex: i];
            if observation.is_null() { continue; }

            let candidates: *mut Object = msg_send![observation, topCandidates: 1usize];
            if candidates.is_null() { continue; }

            let cand_count: usize = msg_send![candidates, count];
            if cand_count == 0 { continue; }

            let candidate: *mut Object = msg_send![candidates, objectAtIndex: 0usize];
            let ns_string: *mut Object = msg_send![candidate, string];
            if !ns_string.is_null() {
                let text = nsstring_to_rust(ns_string);
                if !text.is_empty() {
                    text_parts.push(text);
                }
            }
        }

        Ok(text_parts.join("\n"))
    }

    /// Convert an NSString pointer to a Rust String.
    unsafe fn nsstring_to_rust(ns: *mut Object) -> String {
        if ns.is_null() { return String::new(); }
        let utf8: *const i8 = msg_send![ns, UTF8String];
        if utf8.is_null() { return String::new(); }
        std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned()
    }
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_completion_response_with_tags() {
        let response = "[CORRECTED] Hello world, this is a test.\n[INFERRED] world, test";
        let (corrected, inferred) = parse_completion_response(response);
        assert_eq!(corrected, "Hello world, this is a test.");
        assert_eq!(inferred, vec!["world", "test"]);
    }

    #[test]
    fn parse_completion_response_no_inferred() {
        let response = "[CORRECTED] Exact OCR text.\n[INFERRED] none";
        let (corrected, inferred) = parse_completion_response(response);
        assert_eq!(corrected, "Exact OCR text.");
        assert!(inferred.is_empty());
    }

    #[test]
    fn parse_completion_response_fallback() {
        let response = "Just some raw text without tags";
        let (corrected, inferred) = parse_completion_response(response);
        assert_eq!(corrected, "Just some raw text without tags");
        assert!(inferred.is_empty());
    }

    #[test]
    fn parse_completion_response_multiline_corrected() {
        let response = "[CORRECTED] Line one\nLine two\nLine three\n[INFERRED] none";
        let (corrected, inferred) = parse_completion_response(response);
        assert_eq!(corrected, "Line one\nLine two\nLine three");
        assert!(inferred.is_empty());
    }

    #[test]
    fn capture_region_serialization() {
        let region = CaptureRegion { x: 10.0, y: 20.0, width: 300.0, height: 200.0 };
        let json = serde_json::to_string(&region).unwrap();
        let de: CaptureRegion = serde_json::from_str(&json).unwrap();
        assert_eq!(de.x, 10.0);
        assert_eq!(de.width, 300.0);
    }

    #[test]
    fn vision_capture_chunk_serialization() {
        let chunk = VisionCaptureChunk {
            chunk_type: "ocr_result".into(),
            content: Some("Hello".into()),
            is_ai_inferred: Some(false),
        };
        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("\"type\":\"ocr_result\""));
        assert!(json.contains("\"is_ai_inferred\":false"));
    }

    #[test]
    fn vision_capture_chunk_skips_none_fields() {
        let chunk = VisionCaptureChunk {
            chunk_type: "done".into(),
            content: None,
            is_ai_inferred: None,
        };
        let json = serde_json::to_string(&chunk).unwrap();
        assert!(!json.contains("content"));
        assert!(!json.contains("is_ai_inferred"));
    }
}
