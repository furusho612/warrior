//! プログラムの推奨と進行の純粋な規則 (REQ-PRG-003、REQ-PRG-019、REQ-PRG-020)。
//!
//! データベースに依存せず、入力の一覧から推奨と参考を選び、次の日の位置を決める。

/// 推奨の判断に使うプログラムの概要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramChoice {
    /// プログラムの ID。
    pub id: String,
    /// 目的。
    pub goal: String,
    /// レベル。
    pub level: String,
    /// 週あたりの日数。
    pub days_per_week: u32,
    /// 一覧の並び順。
    pub sort_order: i64,
}

/// 推奨と参考の ID。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProgramRecommendation {
    /// 目的、レベル、日数が一致する推奨。
    pub recommended: Vec<String>,
    /// 目的とレベルだけが一致する参考。
    pub references: Vec<String>,
}

/// 目的、レベル、週あたりの日数から推奨と参考を選ぶ。
///
/// 日数が 0 のときは推奨を返さない。日数が一致する候補がないときは、
/// 目的とレベルが一致する候補をすべて参考として返す。並び順は日数、並び順の昇順である。
pub fn recommend(
    choices: &[ProgramChoice],
    goal: &str,
    level: &str,
    days_per_week: u32,
) -> ProgramRecommendation {
    let mut candidates = choices
        .iter()
        .filter(|choice| choice.goal == goal && choice.level == level)
        .collect::<Vec<_>>();
    candidates.sort_by_key(|choice| (choice.days_per_week, choice.sort_order));
    if days_per_week == 0 {
        return ProgramRecommendation {
            recommended: Vec::new(),
            references: candidates
                .into_iter()
                .map(|choice| choice.id.clone())
                .collect(),
        };
    }
    let recommended = candidates
        .iter()
        .filter(|choice| choice.days_per_week == days_per_week)
        .map(|choice| choice.id.clone())
        .collect::<Vec<_>>();
    if recommended.is_empty() {
        ProgramRecommendation {
            recommended: Vec::new(),
            references: candidates
                .into_iter()
                .map(|choice| choice.id.clone())
                .collect(),
        }
    } else {
        ProgramRecommendation {
            recommended,
            references: Vec::new(),
        }
    }
}

/// トレーニングを終了した日の次に表示する位置を返す。
///
/// 位置は 1 から始まり、最後の日の次は 1 に戻る。日数が 0 のときは 1 を返す。
pub fn next_position(position: i64, total_days: i64) -> i64 {
    if total_days <= 0 || position < 1 || position >= total_days {
        1
    } else {
        position + 1
    }
}

#[cfg(test)]
mod tests {
    use super::{next_position, recommend, ProgramChoice};

    fn choice(id: &str, days: u32, sort_order: i64) -> ProgramChoice {
        ProgramChoice {
            id: id.to_owned(),
            goal: "hypertrophy".to_owned(),
            level: "beginner".to_owned(),
            days_per_week: days,
            sort_order,
        }
    }

    #[test]
    fn the_position_wraps_after_the_last_day() {
        assert_eq!(next_position(1, 4), 2);
        assert_eq!(next_position(4, 4), 1);
        assert_eq!(next_position(1, 0), 1);
        assert_eq!(next_position(5, 4), 1);
    }

    #[test]
    fn exact_days_are_recommended_and_a_missing_day_count_is_a_reference() {
        let choices = vec![choice("a", 3, 2), choice("b", 3, 1), choice("c", 4, 1)];
        let exact = recommend(&choices, "hypertrophy", "beginner", 3);
        assert_eq!(exact.recommended, vec!["b", "a"]);
        assert!(exact.references.is_empty());

        let reference = recommend(&choices, "hypertrophy", "beginner", 5);
        assert!(reference.recommended.is_empty());
        assert_eq!(reference.references, vec!["b", "a", "c"]);

        let zero = recommend(&choices, "hypertrophy", "beginner", 0);
        assert!(zero.recommended.is_empty());
        assert_eq!(zero.references, vec!["b", "a", "c"]);
    }
}
