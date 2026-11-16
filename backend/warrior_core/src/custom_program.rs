//! 独自のプログラムの入力の規則 (REQ-PRG-021、REQ-PRG-023)。

/// 日の数の下限。
pub const MIN_DAYS: usize = 2;
/// 日の数の上限。
pub const MAX_DAYS: usize = 7;
/// 名前の長さの上限 (文字数)。
pub const MAX_NAME_CHARS: usize = 100;

/// 名前と日の数が保存できる範囲か。
pub fn valid(name: &str, day_count: usize) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= MAX_NAME_CHARS
        && (MIN_DAYS..=MAX_DAYS).contains(&day_count)
}

#[cfg(test)]
mod tests {
    use super::{valid, MAX_DAYS, MAX_NAME_CHARS, MIN_DAYS};

    #[test]
    fn the_bounds_are_inclusive() {
        assert!(valid("胸と背中", MIN_DAYS));
        assert!(valid("胸と背中", MAX_DAYS));
        assert!(!valid("胸と背中", MIN_DAYS - 1));
        assert!(!valid("胸と背中", MAX_DAYS + 1));
        assert!(!valid("   ", MIN_DAYS));
        assert!(!valid(&"あ".repeat(MAX_NAME_CHARS + 1), MIN_DAYS));
    }
}
