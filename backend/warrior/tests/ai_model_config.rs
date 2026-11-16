//! `AI_MODEL` の暫定既定値の検査 (ADR-018)。
//!
//! 外部の AI に接続せず、production と staging の設定が同じモデルを指すことだけを確かめる。

const EXPECTED: &str = "@cf/meta/llama-4-scout-17b-16e-instruct";

#[test]
fn the_ai_model_is_the_provisional_default_in_every_environment() {
    let config = include_str!("../../../wrangler.toml");
    let values: Vec<&str> = config
        .lines()
        .filter_map(|line| line.trim().strip_prefix("AI_MODEL = "))
        .collect();
    assert_eq!(
        values.len(),
        3,
        "ローカル、production、staging の 3 つが必要"
    );
    for value in values {
        assert_eq!(value, format!("\"{EXPECTED}\""));
    }
    assert!(!config.contains("@cf/qwen/qwen3-30b-a3b-fp8"));

    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ai_eval");
    let files: Vec<_> = std::fs::read_dir(&fixtures)
        .expect("the fixtures must exist")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .collect();
    assert_eq!(
        files.len(),
        8,
        "the fixtures must cover the requested cases"
    );
    for entry in files {
        let body = std::fs::read_to_string(entry.path()).expect("the fixture must be readable");
        let value: serde_json::Value =
            serde_json::from_str(&body).expect("the fixture must be JSON");
        assert!(value["case"].is_string());
        assert!(value["kind"].is_string());
    }
}
