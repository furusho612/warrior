//! デザイントークンの生成のテスト。
//!
//! 対応する要求: `REQ-NFR-012` (部位の色分けとグラフの配色)。

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::Value;
use warrior_token_gen::{generate_css, generate_rust, write_outputs, Tokens};

const TOKENS_JSON: &str = include_str!("../../../docs/design/tokens.json");

fn tokens() -> Tokens {
    Tokens::from_json(TOKENS_JSON).expect("valid tokens")
}

#[test]
fn generates_every_color_for_both_themes() {
    let tokens = tokens();
    let css = generate_css(&tokens);
    let value: Value = serde_json::from_str(TOKENS_JSON).unwrap();

    for theme in ["dark", "light"] {
        for (key, color) in value["color"][theme].as_object().unwrap() {
            let css_key = camel_to_kebab(key);
            assert!(
                css.contains(&format!("--{css_key}: {};", color.as_str().unwrap())),
                "missing color {theme}.{key}"
            );
        }
        for (key, color) in value["partColor"][theme].as_object().unwrap() {
            let css_key = camel_to_kebab(key);
            assert!(
                css.contains(&format!("--part-{css_key}: {};", color.as_str().unwrap())),
                "missing part color {theme}.{key}"
            );
        }
        for (key, color) in value["figure"][theme].as_object().unwrap() {
            let css_key = camel_to_kebab(key);
            if let Some(color) = color.as_str() {
                assert!(
                    css.contains(&format!("--fig-{css_key}: {color};")),
                    "missing figure color {theme}.{key}"
                );
            } else {
                assert!(
                    css.contains(&format!("--fig-{css_key}: {};", color.as_f64().unwrap())),
                    "missing figure number {theme}.{key}"
                );
            }
        }
    }
}

#[test]
fn generates_fonts_and_layout_constants() {
    let tokens = tokens();
    let css = generate_css(&tokens);
    let rust = generate_rust(&tokens);

    for text in [
        "--font-ui: \"Noto Sans JP\"",
        "--font-numeric: \"Barlow Condensed\"",
        "--screen-width: 390px",
        "--screen-height: 844px",
        "--safe-area-top: 54px",
        "--touch-target-min: 44px",
        "--text-size-min: 11px",
    ] {
        assert!(css.contains(text), "missing {text}");
    }
    for text in [
        "pub const FONT_UI_FAMILY: &str = \"Noto Sans JP\";",
        "pub const FONT_UI_WEIGHTS: &[u16] = &[400, 500, 700, 900];",
        "pub const SCREEN_WIDTH: u16 = 390;",
        "pub const TOUCH_TARGET_MIN: u16 = 44;",
        "pub const TEXT_SIZE_MIN: u16 = 11;",
    ] {
        assert!(rust.contains(text), "missing {text}");
    }
}

#[test]
fn generates_all_muscle_units_with_names() {
    let tokens = tokens();
    let rust = generate_rust(&tokens);
    let value: Value = serde_json::from_str(TOKENS_JSON).unwrap();
    let units = value["muscleUnits"].as_object().unwrap();

    assert_eq!(units.len(), 24);
    for (identifier, name) in units {
        assert!(
            rust.contains(&format!("id: \"{identifier}\"")),
            "missing {identifier}"
        );
        assert!(
            rust.contains(&format!("name: \"{}\"", name.as_str().unwrap())),
            "missing name for {identifier}"
        );
    }
}

#[test]
fn generates_typed_constants_for_colors_parts_and_figure() {
    let rust = generate_rust(&tokens());

    assert!(rust.contains("pub mod color {"));
    assert!(rust.contains("pub const BG: &str = \"#14171D\";"));
    assert!(rust.contains("pub const BG: &str = \"#E9ECF0\";"));
    assert!(rust.contains("pub mod part_color {"));
    assert!(rust.contains("pub const BICEPS_FOREARM: &str = \"#7FC09A\";"));
    assert!(rust.contains("pub mod figure {"));
    assert!(rust.contains("pub const RIM_OPACITY: f64 = 0.12;"));
    assert!(rust.contains("pub fn muscle_unit_name(id: &str) -> Option<&'static str>"));
}

#[test]
fn part_colors_are_distinct_in_both_themes() {
    let value: Value = serde_json::from_str(TOKENS_JSON).unwrap();

    for theme in ["dark", "light"] {
        let colors = value["partColor"][theme]
            .as_object()
            .unwrap()
            .values()
            .map(|color| color.as_str().unwrap())
            .collect::<BTreeSet<&str>>();
        assert_eq!(colors.len(), 8, "part colors must be distinct in {theme}");
    }
}

#[test]
fn missing_required_key_fails() {
    let mut value: Value = serde_json::from_str(TOKENS_JSON).unwrap();
    value["color"]["dark"].as_object_mut().unwrap().remove("bg");
    let missing = serde_json::to_string(&value).unwrap();

    let error = Tokens::from_json(&missing).unwrap_err();
    assert!(error.to_string().contains("color is missing bg"), "{error}");
}

#[test]
fn duplicate_muscle_unit_identifier_fails() {
    let duplicated = TOKENS_JSON.replace(
        "\"pec-mid\": \"大胸筋中部\",",
        "\"pec-upper\": \"大胸筋中部\",",
    );
    assert_ne!(duplicated, TOKENS_JSON);

    let error = Tokens::from_json(&duplicated).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("duplicate muscle unit identifier: pec-upper"),
        "{error}"
    );
}

#[test]
fn invalid_color_value_fails() {
    let mut value: Value = serde_json::from_str(TOKENS_JSON).unwrap();
    value["color"]["dark"]["bg"] = Value::String("dark".to_owned());
    let invalid = serde_json::to_string(&value).unwrap();

    let error = Tokens::from_json(&invalid).unwrap_err();
    assert!(
        error.to_string().contains("color.dark.bg is not #RRGGBB"),
        "{error}"
    );
}

#[test]
fn writes_both_generated_files() {
    let base = std::env::temp_dir().join(format!("warrior-token-gen-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let rust_path = base.join("src/tokens.rs");
    let css_path = base.join("public/tokens.css");
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/tokens.json");

    write_outputs(&input, &rust_path, &css_path).unwrap();

    let rust = fs::read_to_string(&rust_path).unwrap();
    let css = fs::read_to_string(&css_path).unwrap();
    assert!(rust.contains("pub const TOUCH_TARGET_MIN: u16 = 44;"));
    assert!(css.contains("--part-chest: #D87C86;"));

    fs::remove_dir_all(&base).unwrap();
}

fn camel_to_kebab(key: &str) -> String {
    let mut converted = String::new();
    for (index, character) in key.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                converted.push('-');
            }
            converted.push(character.to_ascii_lowercase());
        } else {
            converted.push(character);
        }
    }
    converted
}
