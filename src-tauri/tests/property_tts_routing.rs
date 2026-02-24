// Feature: veya-mvp, Property 8: TTS 语言路由
//
// For any language code and corresponding TTS service configurations,
// the TTS client should route requests to the service address configured
// for that language, not to any other language's service address.
//
// Validates: Requirement 3.7

use proptest::prelude::*;
use veya_lib::api_config::ApiProvider;
use veya_lib::retry::RetryPolicy;
use veya_lib::tts_client::{TtsClient, TtsConfig};

/// Strategy to generate a simple language code like "en", "zh", "ja", "fr", etc.
fn lang_code_strategy() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "en".to_string(),
        "zh".to_string(),
        "ja".to_string(),
        "fr".to_string(),
        "de".to_string(),
        "es".to_string(),
        "ko".to_string(),
        "pt".to_string(),
    ])
}

/// Build a TtsConfig for a given language with a unique base_url.
fn make_config(lang: &str, index: usize) -> TtsConfig {
    TtsConfig {
        provider: ApiProvider::Openai,
        base_url: format!("https://tts-{}-{}.example.com", lang, index),
        model_name: "tts-1".to_string(),
        api_key: format!("key-{}", lang),
        language: lang.to_string(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// When a TTS config exists for the requested language, route_url must
    /// return that language's base_url.
    #[test]
    fn routes_to_exact_language_match(
        target_lang in lang_code_strategy(),
        other_langs in prop::collection::hash_set(lang_code_strategy(), 1..4),
    ) {
        // Build configs: one for the target language, plus others.
        let mut configs: Vec<TtsConfig> = Vec::new();
        // Add other-language configs first so the target isn't just "first".
        for (i, lang) in other_langs.iter().enumerate() {
            if lang != &target_lang {
                configs.push(make_config(lang, i));
            }
        }
        // Add the target language config.
        configs.push(make_config(&target_lang, 99));

        let client = TtsClient::new(configs, RetryPolicy::new(0, 100, 1000));
        let routed_url = client.route_url(&target_lang).unwrap();

        let expected_url = format!("https://tts-{}-99.example.com", target_lang);
        prop_assert_eq!(routed_url, expected_url);
    }

    /// When multiple languages are configured, routing for language A must
    /// NOT return language B's URL (isolation check).
    #[test]
    fn does_not_route_to_wrong_language(
        lang_a in prop::sample::select(vec!["en", "zh", "ja", "fr"]),
        lang_b in prop::sample::select(vec!["de", "es", "ko", "pt"]),
    ) {
        // lang_a and lang_b are always distinct due to disjoint pools.
        let configs = vec![
            make_config(lang_a, 1),
            make_config(lang_b, 2),
        ];

        let client = TtsClient::new(configs, RetryPolicy::new(0, 100, 1000));

        let url_a = client.route_url(lang_a).unwrap();
        let url_b = client.route_url(lang_b).unwrap();

        prop_assert!(url_a.contains(lang_a));
        prop_assert!(url_b.contains(lang_b));
        prop_assert_ne!(url_a, url_b);
    }

    /// Prefix matching: requesting "en-US" should match a config for "en".
    #[test]
    fn routes_via_prefix_match(
        base_lang in prop::sample::select(vec!["en", "zh", "ja", "fr"]),
        suffix in prop::sample::select(vec!["-US", "-GB", "-CN", "-TW", "-JP"]),
    ) {
        let full_lang = format!("{}{}", base_lang, suffix);
        let configs = vec![make_config(base_lang, 1)];

        let client = TtsClient::new(configs, RetryPolicy::new(0, 100, 1000));
        let routed_url = client.route_url(&full_lang).unwrap();

        let expected_url = format!("https://tts-{}-1.example.com", base_lang);
        prop_assert_eq!(routed_url, expected_url);
    }

    /// When no matching language exists, falls back to the first config.
    #[test]
    fn falls_back_to_first_config_when_no_match(
        unknown_lang in "[a-z]{2,3}".prop_filter(
            "must not match any configured lang",
            |s| !["en", "zh"].contains(&s.as_str())
        ),
    ) {
        let configs = vec![
            make_config("en", 1),
            make_config("zh", 2),
        ];

        let client = TtsClient::new(configs, RetryPolicy::new(0, 100, 1000));
        let routed_url = client.route_url(&unknown_lang).unwrap();

        // Should fall back to the first config ("en").
        let expected_url = "https://tts-en-1.example.com".to_string();
        prop_assert_eq!(routed_url, expected_url);
    }
}
