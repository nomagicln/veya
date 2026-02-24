// Feature: veya-mvp, Property 1: 语言检测准确性
//
// For any non-empty text input, the language detection function should return
// a valid, non-empty language code string. For known-language text the result
// should match the expected language.
//
// Validates: Requirement 1.1

use proptest::prelude::*;
use veya_lib::text_insight::detect_language;

fn is_valid_language_code(code: &str) -> bool {
    // Accept the explicit mapping codes, "unknown", or any non-empty
    // alphabetic string (whatlang's Lang::code() returns ISO 639-3 codes).
    !code.is_empty() && code.chars().all(|c| c.is_ascii_alphabetic())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Any non-empty string should produce a non-empty, alphabetic language code.
    #[test]
    fn detect_language_returns_valid_code(text in ".{1,500}") {
        let code = detect_language(&text);
        prop_assert!(
            is_valid_language_code(&code),
            "detect_language returned invalid code '{}' for input '{}'",
            code,
            &text[..text.len().min(80)]
        );
    }

    /// English sentences should be detected as "en".
    #[test]
    fn detect_language_english_text(
        sentence in prop::sample::select(vec![
            "The quick brown fox jumps over the lazy dog",
            "Programming is the art of telling a computer what to do",
            "Language learning requires consistent practice every day",
            "The weather forecast predicts rain for tomorrow afternoon",
            "Scientists discovered a new species in the deep ocean",
        ])
    ) {
        let code = detect_language(sentence);
        prop_assert_eq!(
            code.clone(), "en",
            "Expected 'en' for English text '{}', got '{}'",
            sentence, code
        );
    }

    /// Chinese sentences should be detected as "zh".
    #[test]
    fn detect_language_chinese_text(
        sentence in prop::sample::select(vec![
            "今天天气真好，适合出去散步",
            "学习编程需要持续不断的练习",
            "人工智能正在改变我们的生活方式",
            "这本书讲述了一个关于勇气的故事",
            "科学家们在深海中发现了新物种",
        ])
    ) {
        let code = detect_language(sentence);
        prop_assert_eq!(
            code.clone(), "zh",
            "Expected 'zh' for Chinese text '{}', got '{}'",
            sentence, code
        );
    }

    /// Empty string should return "unknown".
    #[test]
    fn detect_language_empty_is_unknown(_dummy in Just(())) {
        let code = detect_language("");
        prop_assert_eq!(code, "unknown");
    }
}
