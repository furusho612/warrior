//! 部位区分ごとの回復状況の性質テスト。
//!
//! 対応する要求: `REQ-ANL-008`、`REQ-ANL-009`。
//! 任意の最後のトレーニングの時刻と表示の時点の時刻に対して、状態が経過時間と 48 時間、
//! 72 時間の境界から一意に決まること、部位ごとに最後のトレーニングの時刻を使い、
//! その他を回復状況に含めないことを確かめる。

use proptest::prelude::*;
use warrior_core::exercise::BodyPart;
use warrior_core::jst::{JstDate, UtcInstant};
use warrior_core::recovery::{
    last_training_instant, recovery_state, recovery_status, RecoveryState, RecoveryTarget,
    RECOVERY_LOWER_SECONDS, RECOVERY_UPPER_SECONDS,
};

const HOUR: i64 = 60 * 60;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// 状態が、経過時間と 48 時間、72 時間の境界から一意に決まる。
    #[test]
    fn req_anl_008_the_state_follows_the_elapsed_time(
        last_seconds in -2_000_000_000i64..2_000_000_000,
        elapsed_seconds in -96 * HOUR..=144 * HOUR,
    ) {
        let last = UtcInstant::from_epoch_seconds(last_seconds);
        let now = UtcInstant::from_epoch_seconds(last_seconds + elapsed_seconds);
        let target = RecoveryTarget::of(BodyPart::Chest).expect("a chest target");

        prop_assert_eq!(
            recovery_state(now, last, target),
            expected_state(elapsed_seconds),
            "{} seconds after the last training",
            elapsed_seconds
        );
    }

    /// 部位ごとに最後のトレーニングの時刻を使い、その他を回復状況に含めない。
    #[test]
    fn req_anl_008_the_status_uses_the_latest_training_of_each_part(
        now_seconds in -2_000_000_000i64..2_000_000_000,
        trainings in prop::collection::vec(
            (body_part(), -2_000_000_000i64..2_000_000_000),
            0..=40,
        ),
    ) {
        let now = UtcInstant::from_epoch_seconds(now_seconds);
        let trainings = trainings
            .iter()
            .map(|(body_part, seconds)| (*body_part, UtcInstant::from_epoch_seconds(*seconds)))
            .collect::<Vec<_>>();
        let status = recovery_status(now, &trainings);

        prop_assert_eq!(status.parts.len(), BodyPart::ALL.len() - 1);
        for body_part in BodyPart::ALL {
            if body_part.is_other() {
                prop_assert!(
                    status.parts.iter().all(|entry| entry.body_part != BodyPart::Other),
                    "the other part must be excluded"
                );
                continue;
            }
            let entry = status
                .parts
                .iter()
                .find(|entry| entry.body_part == body_part)
                .expect("every part except other has an entry");
            let latest = trainings
                .iter()
                .filter(|(trained, _)| *trained == body_part)
                .map(|(_, instant)| *instant)
                .max();
            prop_assert_eq!(
                entry.last_trained_at.clone(),
                latest.map(UtcInstant::to_iso8601),
                "{:?}",
                body_part
            );
            prop_assert_eq!(
                entry.state,
                latest.map(|latest| expected_state(now.seconds() - latest.seconds())),
                "{:?}",
                body_part
            );
        }
    }

    /// 日付だけの記録の時刻が、その日の翌日の 0 時 (日本時間) になる。
    #[test]
    fn req_anl_008_a_date_only_record_uses_the_next_midnight_in_japan(date in date()) {
        let next = date.add_days(1).expect("a representable next day");
        prop_assert_eq!(last_training_instant(date, None), Some(next.start_utc()));

        let finished_at =
            UtcInstant::from_epoch_seconds(date.start_utc().seconds() + 12 * HOUR);
        prop_assert_eq!(
            last_training_instant(date, Some(finished_at)),
            Some(finished_at)
        );
    }
}

fn expected_state(elapsed_seconds: i64) -> RecoveryState {
    if elapsed_seconds < RECOVERY_LOWER_SECONDS {
        RecoveryState::Recovering
    } else if elapsed_seconds < RECOVERY_UPPER_SECONDS {
        RecoveryState::AlmostRecovered
    } else {
        RecoveryState::Recovered
    }
}

fn date() -> impl Strategy<Value = JstDate> {
    (1900i64..=2100, 1u32..=12, 1u32..=28)
        .prop_map(|(year, month, day)| JstDate::new(year, month, day).expect("a valid date"))
}

fn body_part() -> impl Strategy<Value = BodyPart> {
    prop::sample::select(BodyPart::ALL.to_vec())
}
