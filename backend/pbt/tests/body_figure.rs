//! 人体図の色分けの性質テスト。
//!
//! 対応する要求: `REQ-LIB-003`、`REQ-LIB-012`。
//! 任意の主働筋と補助筋の単位の集合に対して、`data-primary` と `data-secondary` の
//! 値がその集合と一致し、色を付ける要素の集合がその集合の和と一致することを確かめる。

use std::collections::BTreeSet;

use proptest::prelude::*;
use warrior_core::exercise::MuscleUnit;
use warrior_frontend::body_figure::{figure_svg, unit_attribute, FigureMuscles, FigureSide};

/// 人体図の 1 つの単位に付ける役割。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Rest,
    Primary,
    Secondary,
}

fn role() -> impl Strategy<Value = Role> {
    prop::sample::select(vec![Role::Rest, Role::Primary, Role::Secondary])
}

fn muscle_roles() -> impl Strategy<Value = (Vec<MuscleUnit>, Vec<MuscleUnit>)> {
    prop::collection::vec(role(), MuscleUnit::ALL.len()).prop_map(|roles| {
        let mut primary = Vec::new();
        let mut secondary = Vec::new();
        for (unit, role) in MuscleUnit::ALL.into_iter().zip(roles) {
            match role {
                Role::Rest => {}
                Role::Primary => primary.push(unit),
                Role::Secondary => secondary.push(unit),
            }
        }
        (primary, secondary)
    })
}

proptest! {
    #[test]
    fn req_lib_012_colored_elements_match_the_muscle_units(
        (primary, secondary) in muscle_roles()
    ) {
        let expected_primary = unit_set(&primary);
        let expected_secondary = unit_set(&secondary);
        let expected = expected_primary
            .union(&expected_secondary)
            .cloned()
            .collect::<BTreeSet<String>>();
        for side in [FigureSide::Front, FigureSide::Back] {
            let markup = figure_svg(
                side,
                &FigureMuscles {
                    primary: primary.clone(),
                    secondary: secondary.clone(),
                },
            );
            prop_assert_eq!(
                tokens(&attribute(&markup, "data-primary")),
                expected_primary.clone()
            );
            prop_assert_eq!(
                tokens(&attribute(&markup, "data-secondary")),
                expected_secondary.clone()
            );
        }
        let front = figure_svg(
            FigureSide::Front,
            &FigureMuscles {
                primary: primary.clone(),
                secondary: secondary.clone(),
            },
        );
        let back = figure_svg(
            FigureSide::Back,
            &FigureMuscles {
                primary,
                secondary,
            },
        );
        let mut elements = element_units(&front);
        elements.extend(element_units(&back));
        prop_assert_eq!(elements.len(), MuscleUnit::ALL.len());
        let colored = elements
            .iter()
            .map(|unit| (*unit).to_owned())
            .filter(|unit| expected.contains(unit))
            .collect::<BTreeSet<String>>();
        prop_assert_eq!(colored, expected.clone());
    }
}

fn unit_set(units: &[MuscleUnit]) -> BTreeSet<String> {
    units
        .iter()
        .map(|unit| unit_attribute(*unit).to_owned())
        .collect::<BTreeSet<_>>()
}

fn attribute(markup: &str, name: &str) -> String {
    let prefix = format!("{name}=\"");
    let start = markup
        .find(&prefix)
        .unwrap_or_else(|| panic!("{name} must be set"))
        + prefix.len();
    let rest = &markup[start..];
    let end = rest.find('"').expect("the attribute must close");
    rest[..end].to_owned()
}

fn tokens(value: &str) -> BTreeSet<String> {
    value.split_whitespace().map(str::to_owned).collect()
}

fn element_units(markup: &str) -> BTreeSet<&str> {
    let body = markup
        .split_once("</style>")
        .expect("the figure must have a style")
        .1;
    body.match_indices("data-unit=\"")
        .map(|(index, prefix)| {
            let rest = &body[index + prefix.len()..];
            let end = rest.find('"').expect("the attribute must close");
            &rest[..end]
        })
        .collect()
}
