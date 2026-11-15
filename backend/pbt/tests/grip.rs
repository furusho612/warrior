//! グリップの比較キーの性質テスト。
//!
//! 対応する要求: `REQ-SET-011`、`REQ-LIB-011`。
//! 握り方と手幅、アタッチメントと順手か逆手かの任意の組について、比較のキーが
//! 同じ値の組だけを一致させ、違う値の組を一致させないことを確かめる。

use proptest::prelude::*;
use warrior_core::exercise::{Attachment, GripSelection, GripStyle, GripWidth};

fn grip_style() -> impl Strategy<Value = GripStyle> {
    prop::sample::select(GripStyle::ALL.to_vec())
}

fn grip_width() -> impl Strategy<Value = GripWidth> {
    prop::sample::select(GripWidth::ALL.to_vec())
}

fn attachment() -> impl Strategy<Value = Attachment> {
    prop::sample::select(Attachment::ALL.to_vec())
}

fn optional_style() -> impl Strategy<Value = Option<GripStyle>> {
    prop::option::of(grip_style())
}

fn grip_selection() -> impl Strategy<Value = GripSelection> {
    prop_oneof![
        Just(GripSelection::None),
        (grip_style(), grip_width())
            .prop_map(|(style, width)| GripSelection::Grip { style, width }),
        (attachment(), optional_style())
            .prop_map(|(attachment, style)| GripSelection::Attachment { attachment, style }),
    ]
}

fn selection_pair() -> impl Strategy<Value = (GripSelection, GripSelection)> {
    grip_selection().prop_flat_map(|left| {
        prop_oneof![Just(left), grip_selection()].prop_map(move |right| (left, right))
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn the_comparison_key_matches_only_the_same_selection(
        (left, right) in selection_pair(),
    ) {
        prop_assert_eq!(
            left.comparison_key() == right.comparison_key(),
            left == right
        );
    }
}
