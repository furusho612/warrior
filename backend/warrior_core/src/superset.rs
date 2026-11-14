//! スーパーセットの組の種目と、レストタイマーを起動する種目の判定。
//!
//! 対応する要求: `REQ-SET-005`。
//! 組は種目の行の ID を順序付きで持つ。組の最後の種目のセットを完了したときにだけ
//! レストタイマーを起動し、組に属さない種目はセットの完了のたびに起動する。
//! 種目の削除と並べ替えの後は組を種目の順序に合わせ、2 つ未満になった組を解散する。

/// 組の種目の ID の並び。空の組と、同じ種目を複数持つ組は作れない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupersetMembers(Vec<String>);

/// 組の種目の並びを作れない理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupersetMembersError {
    /// 種目が無い。
    Empty,
    /// 同じ種目が複数ある。
    Duplicate,
}

impl SupersetMembers {
    /// 種目の ID の並びから組を作る。
    pub fn new(member_ids: Vec<String>) -> Result<Self, SupersetMembersError> {
        if member_ids.is_empty() {
            return Err(SupersetMembersError::Empty);
        }
        let mut seen = Vec::with_capacity(member_ids.len());
        for id in &member_ids {
            if seen.contains(id) {
                return Err(SupersetMembersError::Duplicate);
            }
            seen.push(id.clone());
        }
        Ok(Self(member_ids))
    }

    /// 組の種目の ID の並び。
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    /// 組の種目の数。
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 組の種目が空か。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 組の最後の種目の ID。
    pub fn last(&self) -> Option<&str> {
        self.0.last().map(String::as_str)
    }

    /// この種目のセットを完了したときに、レストタイマーを起動するか。
    pub fn rest_timer_starts(&self, training_exercise_id: &str) -> bool {
        self.last() == Some(training_exercise_id)
    }

    /// 種目の並びに合わせた組を返す。組の種目が 2 つ未満になるときは解散する。
    ///
    /// 残る種目は `exercise_order` の順に並べる。削除された種目と、ほかの
    /// トレーニングの種目は組から除く。
    pub fn after_exercise_change(&self, exercise_order: &[String]) -> Option<Self> {
        let members = exercise_order
            .iter()
            .filter(|id| self.0.contains(id))
            .cloned()
            .collect::<Vec<_>>();
        (members.len() >= 2).then_some(Self(members))
    }
}

/// この種目のセットを完了したときに、レストタイマーを起動するか。
///
/// 組に属さない種目 (`None`) と、組の種目が空の場合は毎回起動する。
pub fn rest_timer_starts(members: Option<&[String]>, training_exercise_id: &str) -> bool {
    match members {
        None => true,
        Some([]) => true,
        Some(members) => members
            .last()
            .is_some_and(|last| last == training_exercise_id),
    }
}

#[cfg(test)]
mod tests {
    use super::{rest_timer_starts, SupersetMembers, SupersetMembersError};

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn a_group_of_two_starts_the_timer_only_for_the_last_exercise() {
        let members = SupersetMembers::new(ids(&["a", "b"])).expect("a group of two");
        assert!(!members.rest_timer_starts("a"));
        assert!(members.rest_timer_starts("b"));
        assert_eq!(members.last(), Some("b"));
    }

    #[test]
    fn a_group_of_three_keeps_the_order_and_the_last_exercise() {
        let members = SupersetMembers::new(ids(&["a", "b", "c"])).expect("a group of three");
        assert_eq!(members.as_slice(), ids(&["a", "b", "c"]).as_slice());
        assert_eq!(members.len(), 3);
        assert!(!members.rest_timer_starts("b"));
        assert!(members.rest_timer_starts("c"));
    }

    #[test]
    fn an_exercise_outside_a_group_always_starts_the_timer() {
        assert!(rest_timer_starts(None, "a"));
        assert!(rest_timer_starts(Some(&[]), "a"));
        assert!(!rest_timer_starts(Some(&ids(&["a", "b"])), "c"));
    }

    #[test]
    fn removing_the_last_exercise_moves_the_timer_to_the_new_last() {
        let members = SupersetMembers::new(ids(&["a", "b", "c"])).expect("a group of three");
        let updated = members
            .after_exercise_change(&ids(&["a", "b"]))
            .expect("the group keeps two");
        assert_eq!(updated.as_slice(), ids(&["a", "b"]).as_slice());
        assert!(updated.rest_timer_starts("b"));
        assert!(!updated.rest_timer_starts("c"));
    }

    #[test]
    fn reordering_the_exercises_reorders_the_group() {
        let members = SupersetMembers::new(ids(&["a", "b"])).expect("a group of two");
        let updated = members
            .after_exercise_change(&ids(&["b", "a"]))
            .expect("the group keeps two");
        assert_eq!(updated.as_slice(), ids(&["b", "a"]).as_slice());
        assert!(updated.rest_timer_starts("a"));
        assert!(!updated.rest_timer_starts("b"));
    }

    #[test]
    fn one_remaining_exercise_dissolves_the_group() {
        let members = SupersetMembers::new(ids(&["a", "b"])).expect("a group of two");
        assert_eq!(members.after_exercise_change(&ids(&["a"])), None);
        assert_eq!(members.after_exercise_change(&[]), None);
    }

    #[test]
    fn an_empty_group_is_rejected() {
        assert_eq!(
            SupersetMembers::new(Vec::new()),
            Err(SupersetMembersError::Empty)
        );
    }

    #[test]
    fn a_duplicate_exercise_is_rejected() {
        assert_eq!(
            SupersetMembers::new(ids(&["a", "a"])),
            Err(SupersetMembersError::Duplicate)
        );
    }
}
