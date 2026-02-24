// Feature: veya-mvp, Property 10: API 配置独立性与持久化
//
// For any set of API configurations with different model_type values (text,
// vision, tts), each configuration should be independently stored and
// retrievable. After saving a configuration and reading it back, all metadata
// fields (provider, base_url, model_name, language, is_local) must match the
// original values.
//
// Validates: Requirements 5.1, 5.2

use proptest::prelude::*;
use tempfile::TempDir;
use veya_lib::db::Database;

/// Strategy for generating a valid provider string.
fn arb_provider() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("openai"),
        Just("anthropic"),
        Just("elevenlabs"),
        Just("ollama"),
        Just("custom"),
    ]
}

/// Strategy for generating a valid model_type string.
fn arb_model_type() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("text"), Just("vision"), Just("tts"),]
}

/// Strategy for a single API config's fields (excluding model_type, supplied separately).
fn arb_config_fields() -> impl Strategy<Value = (String, String, String, String, String, Option<String>, bool)>
{
    (
        "[a-zA-Z0-9]{1,12}",          // id
        "[a-zA-Z0-9 ]{1,20}",         // name
        arb_provider(),                // provider
        "https?://[a-z]{1,10}\\.com",  // base_url
        "[a-zA-Z0-9-]{1,16}",         // model_name
        prop_oneof![Just(None), "[a-z]{2}(-[A-Z]{2})?".prop_map(Some)], // language
        any::<bool>(),                 // is_local
    )
        .prop_map(|(id, name, provider, base_url, model_name, language, is_local)| {
            (id, name, provider.to_string(), base_url, model_name, language, is_local)
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]

    /// Saving a config and reading it back should preserve all fields.
    #[test]
    fn api_config_roundtrip_preserves_fields(
        model_type in arb_model_type(),
        (id, name, provider, base_url, model_name, language, is_local) in arb_config_fields(),
    ) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path().to_path_buf()).unwrap();
        let api_key_ref = format!("api_key_{id}");

        db.insert_api_config(
            &id, &name, &provider, model_type, &base_url,
            &model_name, &api_key_ref, language.as_deref(), is_local,
        ).unwrap();

        let configs = db.get_api_configs().unwrap();
        let row = configs.iter().find(|c| c.id == id).expect("config must exist");

        prop_assert_eq!(&row.name, &name);
        prop_assert_eq!(&row.provider, &provider);
        prop_assert_eq!(&row.model_type, model_type);
        prop_assert_eq!(&row.base_url, &base_url);
        prop_assert_eq!(&row.model_name, &model_name);
        prop_assert_eq!(&row.api_key_ref, &api_key_ref);
        prop_assert_eq!(&row.language, &language);
        prop_assert_eq!(row.is_local, is_local);
    }

    /// Configs with different model_types are stored independently and don't interfere.
    #[test]
    fn api_configs_independent_by_model_type(
        (id_t, name_t, prov_t, url_t, model_t, lang_t, local_t) in arb_config_fields(),
        (id_v, name_v, prov_v, url_v, model_v, lang_v, local_v) in arb_config_fields(),
        (id_s, name_s, prov_s, url_s, model_s, lang_s, local_s) in arb_config_fields(),
    ) {
        // Ensure unique IDs by appending the model type suffix.
        let id_text = format!("{id_t}_text");
        let id_vision = format!("{id_v}_vision");
        let id_tts = format!("{id_s}_tts");

        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path().to_path_buf()).unwrap();

        // Insert one config per model_type.
        db.insert_api_config(&id_text, &name_t, &prov_t, "text", &url_t, &model_t, &format!("ref_{id_text}"), lang_t.as_deref(), local_t).unwrap();
        db.insert_api_config(&id_vision, &name_v, &prov_v, "vision", &url_v, &model_v, &format!("ref_{id_vision}"), lang_v.as_deref(), local_v).unwrap();
        db.insert_api_config(&id_tts, &name_s, &prov_s, "tts", &url_s, &model_s, &format!("ref_{id_tts}"), lang_s.as_deref(), local_s).unwrap();

        let configs = db.get_api_configs().unwrap();

        // All three should be present.
        let text_cfg = configs.iter().find(|c| c.id == id_text).expect("text config must exist");
        let vision_cfg = configs.iter().find(|c| c.id == id_vision).expect("vision config must exist");
        let tts_cfg = configs.iter().find(|c| c.id == id_tts).expect("tts config must exist");

        // Each config retains its own model_type.
        prop_assert_eq!(&text_cfg.model_type, "text");
        prop_assert_eq!(&vision_cfg.model_type, "vision");
        prop_assert_eq!(&tts_cfg.model_type, "tts");

        // Each config retains its own fields independently.
        prop_assert_eq!(&text_cfg.name, &name_t);
        prop_assert_eq!(&text_cfg.base_url, &url_t);
        prop_assert_eq!(&vision_cfg.name, &name_v);
        prop_assert_eq!(&vision_cfg.base_url, &url_v);
        prop_assert_eq!(&tts_cfg.name, &name_s);
        prop_assert_eq!(&tts_cfg.base_url, &url_s);

        // Deleting one model_type config should not affect others.
        db.delete_api_config(&id_text).unwrap();
        let remaining = db.get_api_configs().unwrap();
        prop_assert!(remaining.iter().all(|c| c.id != id_text), "text config should be deleted");
        prop_assert!(remaining.iter().any(|c| c.id == id_vision), "vision config should remain");
        prop_assert!(remaining.iter().any(|c| c.id == id_tts), "tts config should remain");
    }
}
