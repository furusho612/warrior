//! 独自のプログラムの入力と進行の性質テスト (REQ-PRG-021、REQ-PRG-023)。

use proptest::prelude::*;
use warrior_core::custom_program::{valid, MAX_DAYS, MAX_NAME_CHARS, MIN_DAYS};
use warrior_core::program::next_position;

proptest! {
    #[test]
    fn a_program_is_valid_exactly_within_the_day_and_name_bounds(
        days in 0usize..=10,
        name in ".{0,120}",
    ) {
        let expected = !name.trim().is_empty()
            && name.chars().count() <= MAX_NAME_CHARS
            && (MIN_DAYS..=MAX_DAYS).contains(&days);
        prop_assert_eq!(valid(&name, days), expected);
    }

    #[test]
    fn the_position_of_a_custom_program_wraps(
        position in 1i64..=7,
        total in 2i64..=7,
    ) {
        let start = 1 + (position - 1) % total;
        let next = next_position(start, total);
        prop_assert!((1..=total).contains(&next));
        prop_assert_eq!(next, if start == total { 1 } else { start + 1 });
    }
}
