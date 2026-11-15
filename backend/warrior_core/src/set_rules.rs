//! ドロップセットの位置の規則 (REQ-SET-003)。
//!
//! ドロップセットは直前のセットに続く段として置く。先頭と、ウォームアップセットの
//! 直後には置けない。記録画面とサーバーはこのモジュールの関数だけを使い、規則を
//! それぞれで実装しない。

use crate::set_calc::SetKind;

/// 直前のセット種別から、ドロップセットを置けるか返す。
///
/// 直前のセットが無い先頭と、ウォームアップセットの直後には置けない。
pub fn drop_set_allowed(previous: Option<SetKind>) -> bool {
    previous.is_some_and(|kind| kind != SetKind::Warmup)
}

/// セット種別の並びがドロップセットの位置の規則を満たすか返す。
///
/// 並びは画面の表示順、または記録の順序である。
pub fn drop_set_positions_valid(kinds: &[SetKind]) -> bool {
    kinds.iter().enumerate().all(|(index, kind)| {
        *kind != SetKind::Drop || drop_set_allowed(previous_kind(kinds, index))
    })
}

/// 直後のセット種別から、セットを削除できるか返す。
///
/// 直後がドロップセットのときは削除できない。直後のドロップセットが先頭になるためである。
pub fn delete_set_allowed(next: Option<SetKind>) -> bool {
    next != Some(SetKind::Drop)
}

fn previous_kind(kinds: &[SetKind], index: usize) -> Option<SetKind> {
    index
        .checked_sub(1)
        .and_then(|previous| kinds.get(previous))
        .copied()
}

#[cfg(test)]
mod tests {
    //! 対応する要求: `REQ-SET-003`。

    use super::{delete_set_allowed, drop_set_allowed, drop_set_positions_valid};
    use crate::set_calc::SetKind;

    #[test]
    fn req_set_003_a_drop_set_cannot_be_first() {
        assert!(!drop_set_allowed(None));
        assert!(!drop_set_positions_valid(&[SetKind::Drop]));
    }

    #[test]
    fn req_set_003_a_drop_set_cannot_follow_a_warmup() {
        assert!(!drop_set_allowed(Some(SetKind::Warmup)));
        assert!(!drop_set_positions_valid(&[SetKind::Warmup, SetKind::Drop]));
    }

    #[test]
    fn req_set_003_a_drop_set_can_follow_a_normal_set() {
        assert!(drop_set_allowed(Some(SetKind::Normal)));
        assert!(drop_set_positions_valid(&[SetKind::Normal, SetKind::Drop]));
    }

    #[test]
    fn req_set_003_a_drop_set_can_follow_a_drop_set() {
        assert!(drop_set_allowed(Some(SetKind::Drop)));
        assert!(drop_set_positions_valid(&[
            SetKind::Normal,
            SetKind::Drop,
            SetKind::Drop
        ]));
    }

    #[test]
    fn a_warmup_after_a_drop_set_is_valid() {
        assert!(drop_set_positions_valid(&[
            SetKind::Normal,
            SetKind::Drop,
            SetKind::Warmup
        ]));
    }

    #[test]
    fn a_later_drop_set_is_invalid_when_its_previous_set_becomes_a_warmup() {
        assert!(!drop_set_positions_valid(&[
            SetKind::Warmup,
            SetKind::Warmup,
            SetKind::Drop
        ]));
    }

    #[test]
    fn an_empty_sequence_is_valid() {
        assert!(drop_set_positions_valid(&[]));
    }

    #[test]
    fn req_set_003_a_set_before_a_drop_set_cannot_be_deleted() {
        assert!(!delete_set_allowed(Some(SetKind::Drop)));
        assert!(delete_set_allowed(None));
        assert!(delete_set_allowed(Some(SetKind::Normal)));
        assert!(delete_set_allowed(Some(SetKind::Warmup)));
    }
}
