//! `docs/design/tokens.json` から CSS と Rust の定数を生成する (ADR-011)。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

/// 生成の入口になるデザイントークン。
#[derive(Debug, Deserialize)]
pub struct Tokens {
    font: Fonts,
    layout: Layout,
    color: ColorThemes,
    #[serde(rename = "partColor")]
    part_color: ColorThemes,
    figure: FigureThemes,
    #[serde(rename = "muscleUnits")]
    muscle_units: MuscleUnits,
}

#[derive(Debug, Deserialize)]
struct Fonts {
    ui: FontFamily,
    numeric: FontFamily,
}

#[derive(Debug, Deserialize)]
struct FontFamily {
    family: String,
    weights: Vec<u16>,
}

#[derive(Debug, Deserialize)]
struct Layout {
    #[serde(rename = "screenWidth")]
    screen_width: u16,
    #[serde(rename = "screenHeight")]
    screen_height: u16,
    #[serde(rename = "safeAreaTop")]
    safe_area_top: u16,
    #[serde(rename = "touchTargetMin")]
    touch_target_min: u16,
    #[serde(rename = "textSizeMin")]
    text_size_min: u16,
}

#[derive(Debug, Deserialize)]
struct ColorThemes {
    dark: BTreeMap<String, String>,
    light: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct FigureThemes {
    dark: BTreeMap<String, FigureValue>,
    light: BTreeMap<String, FigureValue>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FigureValue {
    Color(String),
    Number(f64),
}

/// 筋肉の単位の識別子と日本語名。
#[derive(Debug, Clone)]
pub struct MuscleUnits(Vec<(String, String)>);

impl<'de> Deserialize<'de> for MuscleUnits {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MuscleUnitsVisitor;

        impl<'de> Visitor<'de> for MuscleUnitsVisitor {
            type Value = MuscleUnits;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map of muscle unit identifiers to Japanese names")
            }

            fn visit_map<A>(self, mut access: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut units = Vec::new();
                let mut seen = BTreeSet::new();
                while let Some((identifier, name)) = access.next_entry::<String, String>()? {
                    if !seen.insert(identifier.clone()) {
                        return Err(de::Error::custom(format!(
                            "duplicate muscle unit identifier: {identifier}"
                        )));
                    }
                    units.push((identifier, name));
                }
                Ok(MuscleUnits(units))
            }
        }

        deserializer.deserialize_map(MuscleUnitsVisitor)
    }
}

/// 生成の誤り。
#[derive(Debug)]
pub enum TokenError {
    /// JSON の形式が違う。
    Json(String),
    /// 必須の値が無い、または値の範囲が違う。
    Invalid(String),
    /// 入出力に失敗した。
    Io(String),
}

impl fmt::Display for TokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TokenError::Json(message) => write!(formatter, "invalid tokens JSON: {message}"),
            TokenError::Invalid(message) => write!(formatter, "invalid tokens: {message}"),
            TokenError::Io(message) => write!(formatter, "tokens I/O failed: {message}"),
        }
    }
}

impl std::error::Error for TokenError {}

const REQUIRED_COLOR_KEYS: &[&str] = &[
    "bg",
    "surface",
    "surfaceRaised",
    "line",
    "text",
    "textSub",
    "primary",
    "onPrimary",
    "mark",
    "timerDigits",
    "saved",
];

const REQUIRED_PART_KEYS: &[&str] = &[
    "chest",
    "back",
    "shoulders",
    "bicepsForearm",
    "triceps",
    "legs",
    "abs",
    "other",
];

const REQUIRED_FIGURE_KEYS: &[&str] = &[
    "body",
    "tendon",
    "rest",
    "restLine",
    "primary",
    "primaryLine",
    "secondary",
    "secondaryLine",
    "rim",
    "rimOpacity",
];

impl Tokens {
    /// JSON の文字列からデザイントークンを読み、必須の値を検証する。
    pub fn from_json(text: &str) -> Result<Self, TokenError> {
        let tokens: Tokens =
            serde_json::from_str(text).map_err(|error| TokenError::Json(error.to_string()))?;
        tokens.validate()?;
        Ok(tokens)
    }

    fn validate(&self) -> Result<(), TokenError> {
        validate_font("ui", &self.font.ui)?;
        validate_font("numeric", &self.font.numeric)?;
        validate_layout(&self.layout)?;
        validate_colors("color", &self.color, REQUIRED_COLOR_KEYS)?;
        validate_colors("partColor", &self.part_color, REQUIRED_PART_KEYS)?;
        validate_figure(&self.figure)?;
        validate_muscle_units(&self.muscle_units)?;
        Ok(())
    }
}

fn validate_font(name: &str, font: &FontFamily) -> Result<(), TokenError> {
    if font.family.trim().is_empty() {
        return Err(TokenError::Invalid(format!("font.{name}.family is empty")));
    }
    if font.weights.is_empty() {
        return Err(TokenError::Invalid(format!("font.{name}.weights is empty")));
    }
    if font
        .weights
        .iter()
        .any(|weight| !(1..=1000).contains(weight))
    {
        return Err(TokenError::Invalid(format!(
            "font.{name}.weights must be between 1 and 1000"
        )));
    }
    Ok(())
}

fn validate_layout(layout: &Layout) -> Result<(), TokenError> {
    if layout.screen_width == 0 || layout.screen_height == 0 {
        return Err(TokenError::Invalid("layout screen size is zero".to_owned()));
    }
    if layout.safe_area_top == 0 {
        return Err(TokenError::Invalid(
            "layout.safeAreaTop must be greater than zero".to_owned(),
        ));
    }
    if layout.touch_target_min < 44 {
        return Err(TokenError::Invalid(
            "layout.touchTargetMin must be 44 or more".to_owned(),
        ));
    }
    if layout.text_size_min < 11 {
        return Err(TokenError::Invalid(
            "layout.textSizeMin must be 11 or more".to_owned(),
        ));
    }
    Ok(())
}

fn validate_colors(
    name: &str,
    themes: &ColorThemes,
    required_keys: &[&str],
) -> Result<(), TokenError> {
    validate_theme_keys(name, &themes.dark, required_keys)?;
    validate_theme_keys(name, &themes.light, required_keys)?;
    if themes.dark.keys().ne(themes.light.keys()) {
        return Err(TokenError::Invalid(format!(
            "{name} dark and light keys differ"
        )));
    }
    for (theme, colors) in [("dark", &themes.dark), ("light", &themes.light)] {
        for (key, value) in colors {
            validate_token_key(&format!("{name}.{theme}"), key)?;
            if !is_hex_color(value) {
                return Err(TokenError::Invalid(format!(
                    "{name}.{theme}.{key} is not #RRGGBB"
                )));
            }
        }
    }
    Ok(())
}

fn validate_theme_keys(
    name: &str,
    colors: &BTreeMap<String, String>,
    required_keys: &[&str],
) -> Result<(), TokenError> {
    for key in required_keys {
        if !colors.contains_key(*key) {
            return Err(TokenError::Invalid(format!("{name} is missing {key}")));
        }
    }
    Ok(())
}

fn validate_figure(themes: &FigureThemes) -> Result<(), TokenError> {
    validate_figure_keys("figure.dark", &themes.dark)?;
    validate_figure_keys("figure.light", &themes.light)?;
    if themes.dark.keys().ne(themes.light.keys()) {
        return Err(TokenError::Invalid(
            "figure dark and light keys differ".to_owned(),
        ));
    }
    for (theme, values) in [("dark", &themes.dark), ("light", &themes.light)] {
        for (key, value) in values {
            validate_token_key(&format!("figure.{theme}"), key)?;
            match (key.as_str(), value) {
                ("rimOpacity", FigureValue::Number(opacity)) if (0.0..=1.0).contains(opacity) => {}
                ("rimOpacity", _) => {
                    return Err(TokenError::Invalid(
                        "figure.rimOpacity must be between 0 and 1".to_owned(),
                    ))
                }
                (_, FigureValue::Color(color)) if is_hex_color(color) => {}
                (_, FigureValue::Color(_)) => {
                    return Err(TokenError::Invalid(format!(
                        "figure.{theme}.{key} is not #RRGGBB"
                    )))
                }
                (_, FigureValue::Number(_)) => {
                    return Err(TokenError::Invalid(format!(
                        "figure.{theme}.{key} must be a color"
                    )))
                }
            }
        }
    }
    Ok(())
}

fn validate_figure_keys(
    name: &str,
    values: &BTreeMap<String, FigureValue>,
) -> Result<(), TokenError> {
    for key in REQUIRED_FIGURE_KEYS {
        if !values.contains_key(*key) {
            return Err(TokenError::Invalid(format!("{name} is missing {key}")));
        }
    }
    Ok(())
}

fn validate_muscle_units(units: &MuscleUnits) -> Result<(), TokenError> {
    if units.0.is_empty() {
        return Err(TokenError::Invalid("muscleUnits is empty".to_owned()));
    }
    for (identifier, name) in &units.0 {
        if !is_kebab_identifier(identifier) {
            return Err(TokenError::Invalid(format!(
                "muscle unit identifier is invalid: {identifier}"
            )));
        }
        if name.trim().is_empty() {
            return Err(TokenError::Invalid(format!(
                "muscle unit name is empty: {identifier}"
            )));
        }
    }
    Ok(())
}

fn validate_token_key(scope: &str, key: &str) -> Result<(), TokenError> {
    let mut characters = key.chars();
    let Some(first) = characters.next() else {
        return Err(TokenError::Invalid(format!("{scope} has an empty key")));
    };
    if !first.is_ascii_lowercase() || !characters.all(|character| character.is_ascii_alphanumeric())
    {
        return Err(TokenError::Invalid(format!(
            "{scope} has an invalid key: {key}"
        )));
    }
    Ok(())
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn is_kebab_identifier(value: &str) -> bool {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    characters.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    })
}

/// 読み込んだデザイントークンから CSS のカスタムプロパティを生成する。
pub fn generate_css(tokens: &Tokens) -> String {
    let mut css = String::new();
    css.push_str("/* docs/design/tokens.json から生成した。手で編集しない。 */\n\n");
    css.push_str(":root {\n");
    write_fonts(&mut css, tokens, "  ");
    write_layout(&mut css, &tokens.layout, "  ");
    css.push_str("}\n\n");

    write_theme_block(
        &mut css,
        ":root, [data-theme=\"dark\"]",
        "dark",
        &tokens.color.dark,
        &tokens.part_color.dark,
        &tokens.figure.dark,
    );
    write_theme_block(
        &mut css,
        "[data-theme=\"light\"]",
        "light",
        &tokens.color.light,
        &tokens.part_color.light,
        &tokens.figure.light,
    );

    css.push_str("@media (prefers-color-scheme: light) {\n");
    write_theme_block(
        &mut css,
        "  :root:not([data-theme])",
        "light",
        &tokens.color.light,
        &tokens.part_color.light,
        &tokens.figure.light,
    );
    css.push_str("}\n");
    css
}

fn write_fonts(css: &mut String, tokens: &Tokens, indent: &str) {
    css.push_str(&format!(
        "{indent}--font-ui: \"{}\", system-ui, sans-serif;\n",
        tokens.font.ui.family
    ));
    css.push_str(&format!(
        "{indent}--font-numeric: \"{}\", sans-serif;\n",
        tokens.font.numeric.family
    ));
}

fn write_layout(css: &mut String, layout: &Layout, indent: &str) {
    css.push_str(&format!(
        "{indent}--screen-width: {}px;\n",
        layout.screen_width
    ));
    css.push_str(&format!(
        "{indent}--screen-height: {}px;\n",
        layout.screen_height
    ));
    css.push_str(&format!(
        "{indent}--safe-area-top: {}px;\n",
        layout.safe_area_top
    ));
    css.push_str(&format!(
        "{indent}--touch-target-min: {}px;\n",
        layout.touch_target_min
    ));
    css.push_str(&format!(
        "{indent}--text-size-min: {}px;\n",
        layout.text_size_min
    ));
}

fn write_theme_block(
    css: &mut String,
    selector: &str,
    color_scheme: &str,
    colors: &BTreeMap<String, String>,
    part_colors: &BTreeMap<String, String>,
    figure: &BTreeMap<String, FigureValue>,
) {
    css.push_str(&format!("{selector} {{\n"));
    css.push_str(&format!("  color-scheme: {color_scheme};\n"));
    for (key, value) in colors {
        css.push_str(&format!("  --{}: {};\n", to_kebab_case(key), value));
    }
    for (key, value) in part_colors {
        css.push_str(&format!("  --part-{}: {};\n", to_kebab_case(key), value));
    }
    for (key, value) in figure {
        let css_value = match value {
            FigureValue::Color(color) => color.clone(),
            FigureValue::Number(number) => number.to_string(),
        };
        css.push_str(&format!("  --fig-{}: {};\n", to_kebab_case(key), css_value));
    }
    css.push_str("}\n\n");
}

/// 読み込んだデザイントークンから Rust の型付き定数を生成する。
pub fn generate_rust(tokens: &Tokens) -> String {
    let mut rust = String::new();
    rust.push_str("//! docs/design/tokens.json から生成した定数。手で編集しない。\n\n");

    write_color_module(&mut rust, "color", &tokens.color.dark, &tokens.color.light);
    write_color_module(
        &mut rust,
        "part_color",
        &tokens.part_color.dark,
        &tokens.part_color.light,
    );
    write_figure_module(&mut rust, &tokens.figure.dark, &tokens.figure.light);

    rust.push_str(&format!(
        "pub const FONT_UI_FAMILY: &str = {:?};\n",
        tokens.font.ui.family
    ));
    write_weights(&mut rust, "FONT_UI_WEIGHTS", &tokens.font.ui.weights);
    rust.push_str(&format!(
        "pub const FONT_NUMERIC_FAMILY: &str = {:?};\n",
        tokens.font.numeric.family
    ));
    write_weights(
        &mut rust,
        "FONT_NUMERIC_WEIGHTS",
        &tokens.font.numeric.weights,
    );

    rust.push_str(&format!(
        "pub const SCREEN_WIDTH: u16 = {};\n",
        tokens.layout.screen_width
    ));
    rust.push_str(&format!(
        "pub const SCREEN_HEIGHT: u16 = {};\n",
        tokens.layout.screen_height
    ));
    rust.push_str(&format!(
        "pub const SAFE_AREA_TOP: u16 = {};\n",
        tokens.layout.safe_area_top
    ));
    rust.push_str(&format!(
        "pub const TOUCH_TARGET_MIN: u16 = {};\n",
        tokens.layout.touch_target_min
    ));
    rust.push_str(&format!(
        "pub const TEXT_SIZE_MIN: u16 = {};\n\n",
        tokens.layout.text_size_min
    ));

    rust.push_str(
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub struct MuscleUnit {\n    pub id: &'static str,\n    pub name: &'static str,\n}\n\n",
    );
    rust.push_str("pub const MUSCLE_UNITS: &[MuscleUnit] = &[\n");
    for (identifier, name) in &tokens.muscle_units.0 {
        rust.push_str(&format!(
            "    MuscleUnit {{\n        id: {:?},\n        name: {:?},\n    }},\n",
            identifier, name
        ));
    }
    rust.push_str("];\n\n");
    rust.push_str(
        "pub fn muscle_unit_name(id: &str) -> Option<&'static str> {\n    MUSCLE_UNITS\n        .iter()\n        .find(|unit| unit.id == id)\n        .map(|unit| unit.name)\n}\n",
    );
    rust
}

fn write_color_module(
    rust: &mut String,
    module: &str,
    dark: &BTreeMap<String, String>,
    light: &BTreeMap<String, String>,
) {
    rust.push_str(&format!("pub mod {module} {{\n"));
    write_color_theme(rust, "dark", dark);
    write_color_theme(rust, "light", light);
    rust.push_str("}\n\n");
}

fn write_color_theme(rust: &mut String, theme: &str, colors: &BTreeMap<String, String>) {
    rust.push_str(&format!("    pub mod {theme} {{\n"));
    for (key, value) in colors {
        rust.push_str(&format!(
            "        pub const {}: &str = {:?};\n",
            to_screaming_snake(key),
            value
        ));
    }
    rust.push_str("    }\n");
}

fn write_figure_module(
    rust: &mut String,
    dark: &BTreeMap<String, FigureValue>,
    light: &BTreeMap<String, FigureValue>,
) {
    rust.push_str("pub mod figure {\n");
    write_figure_theme(rust, "dark", dark);
    write_figure_theme(rust, "light", light);
    rust.push_str("}\n\n");
}

fn write_figure_theme(rust: &mut String, theme: &str, figure: &BTreeMap<String, FigureValue>) {
    rust.push_str(&format!("    pub mod {theme} {{\n"));
    for (key, value) in figure {
        let constant = to_screaming_snake(key);
        match value {
            FigureValue::Color(color) => rust.push_str(&format!(
                "        pub const {constant}: &str = {:?};\n",
                color
            )),
            FigureValue::Number(number) => {
                rust.push_str(&format!("        pub const {constant}: f64 = {number};\n"))
            }
        }
    }
    rust.push_str("    }\n");
}

fn write_weights(rust: &mut String, name: &str, weights: &[u16]) {
    let values = weights
        .iter()
        .map(u16::to_string)
        .collect::<Vec<String>>()
        .join(", ");
    rust.push_str(&format!("pub const {name}: &[u16] = &[{values}];\n"));
}

fn to_kebab_case(key: &str) -> String {
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

fn to_screaming_snake(key: &str) -> String {
    to_kebab_case(key).replace('-', "_").to_ascii_uppercase()
}

/// 入力の JSON を読み、Rust と CSS の生成物を書き出す。
pub fn write_outputs(
    input: &Path,
    rust_output: &Path,
    css_output: &Path,
) -> Result<(), TokenError> {
    let text = fs::read_to_string(input)
        .map_err(|error| TokenError::Io(format!("{}: {error}", input.display())))?;
    let tokens = Tokens::from_json(&text)?;

    write_file(rust_output, &generate_rust(&tokens))?;
    write_file(css_output, &generate_css(&tokens))?;
    Ok(())
}

fn write_file(path: &Path, contents: &str) -> Result<(), TokenError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| TokenError::Io(format!("{}: {error}", parent.display())))?;
    }
    fs::write(path, contents)
        .map_err(|error| TokenError::Io(format!("{}: {error}", path.display())))
}

/// リポジトリの既定の場所で生成する。
pub fn write_default_outputs() -> Result<(), TokenError> {
    write_outputs(
        Path::new("docs/design/tokens.json"),
        Path::new("frontend/src/generated/tokens.rs"),
        Path::new("frontend/public/generated/tokens.css"),
    )
}

#[cfg(test)]
mod tests {
    use super::{to_kebab_case, to_screaming_snake};

    #[test]
    fn converts_camel_case_keys_to_css_names() {
        assert_eq!(to_kebab_case("surfaceRaised"), "surface-raised");
        assert_eq!(to_kebab_case("bicepsForearm"), "biceps-forearm");
        assert_eq!(to_kebab_case("rimOpacity"), "rim-opacity");
    }

    #[test]
    fn converts_camel_case_keys_to_rust_names() {
        assert_eq!(to_screaming_snake("surfaceRaised"), "SURFACE_RAISED");
        assert_eq!(to_screaming_snake("onPrimary"), "ON_PRIMARY");
        assert_eq!(to_screaming_snake("rimOpacity"), "RIM_OPACITY");
    }
}
