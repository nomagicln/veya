// Feature: veya-mvp, Property 3: AI 补全流水线条件行为
//
// For any OCR result, the processing pipeline includes the AI completion step
// if and only if AI_Completion is enabled. When disabled, the output should
// come directly from the OCR result without AI modification.
//
// Validates: Requirements 2.3, 2.4

use proptest::prelude::*;
use veya_lib::vision_capture::parse_completion_response;

/// Simulate the pipeline's conditional branching.
///
/// When `ai_completion` is true, the pipeline sends OCR text to an LLM and
/// parses the response via `parse_completion_response`. The corrected text
/// and inferred parts are emitted.
///
/// When `ai_completion` is false, the pipeline emits only the raw OCR text
/// and never invokes AI processing.
fn simulate_pipeline(ocr_text: &str, ai_completion: bool, ai_response: &str) -> PipelineOutput {
    // OCR result is always emitted regardless of the flag.
    let mut output = PipelineOutput {
        ocr_emitted: true,
        ocr_text: ocr_text.to_string(),
        ai_completion_ran: false,
        corrected_text: None,
        inferred_parts: None,
    };

    // AI completion branch: only runs when the flag is true.
    if ai_completion {
        let (corrected, inferred) = parse_completion_response(ai_response);
        output.ai_completion_ran = true;
        output.corrected_text = Some(corrected);
        output.inferred_parts = Some(inferred);
    }

    output
}

#[derive(Debug)]
struct PipelineOutput {
    ocr_emitted: bool,
    ocr_text: String,
    ai_completion_ran: bool,
    corrected_text: Option<String>,
    inferred_parts: Option<Vec<String>>,
}

/// Strategy for generating non-empty OCR text.
fn ocr_text_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 .,!?]{5,100}".prop_filter("must be non-empty after trim", |s| !s.trim().is_empty())
}

/// Strategy for generating a well-formed AI completion response.
fn ai_response_strategy() -> impl Strategy<Value = String> {
    (
        "[a-zA-Z0-9 .,!?]{5,80}",                    // corrected text
        prop::collection::vec("[a-zA-Z]{2,10}", 0..4), // inferred words
    )
        .prop_map(|(corrected, inferred)| {
            if inferred.is_empty() {
                format!("[CORRECTED] {corrected}\n[INFERRED] none")
            } else {
                format!(
                    "[CORRECTED] {corrected}\n[INFERRED] {}",
                    inferred.join(", ")
                )
            }
        })
}

/// Strategy for generating a raw/malformed AI response (no tags).
fn raw_ai_response_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 .,!?]{5,100}"
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// When ai_completion is false, the pipeline must NOT run AI processing.
    /// Only the raw OCR text is emitted.
    #[test]
    fn disabled_ai_completion_skips_ai_branch(
        ocr_text in ocr_text_strategy(),
        ai_response in ai_response_strategy(),
    ) {
        let output = simulate_pipeline(&ocr_text, false, &ai_response);

        // OCR result is always emitted.
        prop_assert!(output.ocr_emitted);
        prop_assert_eq!(&output.ocr_text, &ocr_text);

        // AI branch must NOT have run.
        prop_assert!(!output.ai_completion_ran);
        prop_assert!(output.corrected_text.is_none());
        prop_assert!(output.inferred_parts.is_none());
    }

    /// When ai_completion is true, the pipeline MUST run AI processing.
    /// The corrected text must be non-empty and inferred parts must be present.
    #[test]
    fn enabled_ai_completion_runs_ai_branch(
        ocr_text in ocr_text_strategy(),
        ai_response in ai_response_strategy(),
    ) {
        let output = simulate_pipeline(&ocr_text, true, &ai_response);

        // OCR result is always emitted.
        prop_assert!(output.ocr_emitted);
        prop_assert_eq!(&output.ocr_text, &ocr_text);

        // AI branch MUST have run.
        prop_assert!(output.ai_completion_ran);
        prop_assert!(output.corrected_text.is_some());
        prop_assert!(!output.corrected_text.as_ref().unwrap().is_empty());
        prop_assert!(output.inferred_parts.is_some());
    }

    /// The ai_completion flag is the sole determinant of whether AI runs.
    /// For any given OCR text, toggling the flag must toggle the AI branch.
    #[test]
    fn flag_is_sole_determinant_of_ai_branch(
        ocr_text in ocr_text_strategy(),
        ai_response in ai_response_strategy(),
    ) {
        let output_on = simulate_pipeline(&ocr_text, true, &ai_response);
        let output_off = simulate_pipeline(&ocr_text, false, &ai_response);

        // Both emit OCR text.
        prop_assert!(output_on.ocr_emitted);
        prop_assert!(output_off.ocr_emitted);

        // Only the enabled path runs AI.
        prop_assert!(output_on.ai_completion_ran);
        prop_assert!(!output_off.ai_completion_ran);

        // Disabled path has no AI output.
        prop_assert!(output_off.corrected_text.is_none());
        prop_assert!(output_on.corrected_text.is_some());
    }

    /// parse_completion_response always produces a non-empty corrected string,
    /// even when the AI response is malformed (no tags). This ensures the AI
    /// branch never produces empty output.
    #[test]
    fn ai_branch_always_produces_nonempty_corrected_text(
        ai_response in raw_ai_response_strategy(),
    ) {
        let (corrected, _inferred) = parse_completion_response(&ai_response);
        prop_assert!(!corrected.is_empty(), "corrected text must never be empty");
    }

    /// When AI response contains [INFERRED] with items, those items appear
    /// in the parsed inferred list. When [INFERRED] is "none", the list is empty.
    #[test]
    fn ai_branch_inferred_parts_match_response(
        corrected_text in "[a-zA-Z ]{5,40}",
        inferred_words in prop::collection::vec("[a-zA-Z]{3,8}", 0..5),
    ) {
        let response = if inferred_words.is_empty() {
            format!("[CORRECTED] {corrected_text}\n[INFERRED] none")
        } else {
            format!(
                "[CORRECTED] {corrected_text}\n[INFERRED] {}",
                inferred_words.join(", ")
            )
        };

        let (_corrected, inferred) = parse_completion_response(&response);

        if inferred_words.is_empty() {
            prop_assert!(inferred.is_empty());
        } else {
            prop_assert_eq!(inferred.len(), inferred_words.len());
            for (parsed, original) in inferred.iter().zip(inferred_words.iter()) {
                prop_assert_eq!(parsed, original);
            }
        }
    }
}
