//! 人体図のブラウザのテスト。
//!
//! 対応する要求: `REQ-LIB-003`、`REQ-LIB-012`。
//! 24 個の筋肉の単位のそれぞれに対応する SVG の要素があり、`data-primary` と
//! `data-secondary` の値で色が変わることを、ライトとダークの両方で確かめる。

use warrior_core::exercise::MuscleUnit;
use warrior_frontend::body_figure::{figure_svg, unit_attribute, FigureMuscles, FigureSide};
use warrior_frontend::generated::tokens;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use wasm_bindgen_test::wasm_bindgen_test_configure;

wasm_bindgen_test_configure!(run_in_browser);

/// 人体図の色を決める CSS カスタムプロパティ。
struct FigureColors {
    body: &'static str,
    tendon: &'static str,
    rest: &'static str,
    rest_line: &'static str,
    primary: &'static str,
    primary_line: &'static str,
    secondary: &'static str,
    secondary_line: &'static str,
    rim: &'static str,
    rim_opacity: f64,
}

const DARK: FigureColors = FigureColors {
    body: tokens::figure::dark::BODY,
    tendon: tokens::figure::dark::TENDON,
    rest: tokens::figure::dark::REST,
    rest_line: tokens::figure::dark::REST_LINE,
    primary: tokens::figure::dark::PRIMARY,
    primary_line: tokens::figure::dark::PRIMARY_LINE,
    secondary: tokens::figure::dark::SECONDARY,
    secondary_line: tokens::figure::dark::SECONDARY_LINE,
    rim: tokens::figure::dark::RIM,
    rim_opacity: tokens::figure::dark::RIM_OPACITY,
};

const LIGHT: FigureColors = FigureColors {
    body: tokens::figure::light::BODY,
    tendon: tokens::figure::light::TENDON,
    rest: tokens::figure::light::REST,
    rest_line: tokens::figure::light::REST_LINE,
    primary: tokens::figure::light::PRIMARY,
    primary_line: tokens::figure::light::PRIMARY_LINE,
    secondary: tokens::figure::light::SECONDARY,
    secondary_line: tokens::figure::light::SECONDARY_LINE,
    rim: tokens::figure::light::RIM,
    rim_opacity: tokens::figure::light::RIM_OPACITY,
};

#[wasm_bindgen_test]
fn req_lib_003_and_012_every_muscle_unit_changes_color_with_the_theme() {
    let container = attach_container();

    for colors in [&DARK, &LIGHT] {
        container
            .set_attribute("style", &figure_style(colors))
            .expect("the figure colors must be set");
        for unit in MuscleUnit::ALL {
            let id = unit_attribute(unit);

            container.set_inner_html(&both_sides(&FigureMuscles {
                primary: Vec::new(),
                secondary: Vec::new(),
            }));
            assert_color(&container, &unit_selector("m", id), "fill", colors.rest);
            assert_color(
                &container,
                &unit_selector("f", id),
                "stroke",
                colors.rest_line,
            );

            container.set_inner_html(&both_sides(&FigureMuscles {
                primary: vec![unit],
                secondary: Vec::new(),
            }));
            assert_color(&container, &unit_selector("m", id), "fill", colors.primary);
            assert_color(
                &container,
                &unit_selector("f", id),
                "stroke",
                colors.primary_line,
            );

            container.set_inner_html(&both_sides(&FigureMuscles {
                primary: Vec::new(),
                secondary: vec![unit],
            }));
            assert_color(
                &container,
                &unit_selector("m", id),
                "fill",
                colors.secondary,
            );
            assert_color(
                &container,
                &unit_selector("f", id),
                "stroke",
                colors.secondary_line,
            );
        }
    }

    detach_container(&container);
}

#[wasm_bindgen_test]
fn req_lib_003_the_front_and_back_figures_share_a_page_without_collisions() {
    let container = attach_container();
    container
        .set_attribute("style", &figure_style(&DARK))
        .expect("the figure colors must be set");
    let markup = format!(
        "{}{}",
        figure_svg(FigureSide::Front, &FigureMuscles::default()),
        figure_svg(FigureSide::Back, &FigureMuscles::default())
    );
    container.set_inner_html(&markup);

    let figures = container
        .query_selector_all("svg[data-figure-side]")
        .expect("the figure selector must be valid");
    assert_eq!(figures.length(), 2);

    let elements = container
        .query_selector_all("[id]")
        .expect("the id selector must be valid");
    let mut ids = Vec::new();
    for index in 0..elements.length() {
        let element = elements
            .get(index)
            .expect("the element must exist")
            .unchecked_into::<web_sys::Element>();
        ids.push(element.id());
    }
    assert!(!ids.is_empty());
    for id in &ids {
        assert!(id.starts_with("wfig-"), "{id}");
    }
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ids.len());

    detach_container(&container);
}

fn attach_container() -> web_sys::Element {
    let document = web_sys::window()
        .and_then(|window| window.document())
        .expect("a browser must have a document");
    let container = document
        .create_element("div")
        .expect("a div must be created");
    document
        .body()
        .expect("a browser must have a body")
        .unchecked_ref::<web_sys::Node>()
        .append_child(container.unchecked_ref::<web_sys::Node>())
        .expect("the container must be attached");
    container
}

fn detach_container(container: &web_sys::Element) {
    container
        .parent_node()
        .expect("the container must have a parent")
        .remove_child(container)
        .expect("the container must be removed");
}

fn figure_style(colors: &FigureColors) -> String {
    format!(
        "--fig-body:{};--fig-tendon:{};--fig-rest:{};--fig-rest-line:{};\
         --fig-primary:{};--fig-primary-line:{};--fig-secondary:{};--fig-secondary-line:{};\
         --fig-rim:{};--fig-rim-opacity:{}",
        colors.body,
        colors.tendon,
        colors.rest,
        colors.rest_line,
        colors.primary,
        colors.primary_line,
        colors.secondary,
        colors.secondary_line,
        colors.rim,
        colors.rim_opacity,
    )
}

fn both_sides(muscles: &FigureMuscles) -> String {
    format!(
        "{}{}",
        figure_svg(FigureSide::Front, muscles),
        figure_svg(FigureSide::Back, muscles)
    )
}

fn unit_selector(class: &str, id: &str) -> String {
    format!(r#".wfig-{class}[data-unit="{id}"]"#)
}

fn assert_color(container: &web_sys::Element, selector: &str, property: &str, expected: &str) {
    let elements = container
        .query_selector_all(selector)
        .expect("the unit selector must be valid");
    assert!(elements.length() > 0, "{selector} must exist");
    let window = web_sys::window().expect("a browser must have a window");
    for index in 0..elements.length() {
        let element = elements
            .get(index)
            .expect("the element must exist")
            .unchecked_into::<web_sys::Element>();
        let style = window
            .get_computed_style(&element)
            .expect("the computed style must be readable")
            .expect("the computed style must exist");
        assert_eq!(
            style.get_property_value(property).expect("the property"),
            rgb(expected),
            "{selector} at {index}"
        );
    }
}

fn rgb(hex: &str) -> String {
    let channel = |start: usize| {
        u8::from_str_radix(&hex[start..start + 2], 16).expect("a color channel must be hex")
    };
    format!("rgb({}, {}, {})", channel(1), channel(3), channel(5))
}
