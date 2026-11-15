//! 部位区分ごとの回復状況 (PRD の「回復状況」)。
//!
//! 対応する要求: `REQ-ANL-008`、`REQ-ANL-009`、`REQ-ANL-010`。
//! 最後のトレーニングの時刻は PRD の「計算ルールと用語定義」に従う。日付だけの記録は
//! その日の翌日の 0 時 (日本時間) とし、回復を早く見積もらない。状態の境界は 48 時間と
//! 72 時間で固定し、個人差とトレーニング量による補正は行わない。表示の時点の時刻は
//! 呼び出し側から受け取り、この module は現在時刻を読まない。

use serde::{Deserialize, Serialize};

use crate::exercise::BodyPart;
use crate::jst::{JstDate, UtcInstant};

/// 回復中とほぼ回復の境目の経過時間 (秒)。48 時間。
pub const RECOVERY_LOWER_SECONDS: i64 = 48 * 60 * 60;

/// ほぼ回復と回復済みの境目の経過時間 (秒)。72 時間。
pub const RECOVERY_UPPER_SECONDS: i64 = 72 * 60 * 60;

const NANOS_PER_SECOND: i128 = 1_000_000_000;

/// 回復の段階。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryState {
    /// 目安時間の下限を経過していない。
    Recovering,
    /// 下限は経過し、上限を経過していない。
    AlmostRecovered,
    /// 上限を経過している。
    Recovered,
}

impl RecoveryState {
    /// API と画面の属性に載せる値。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recovering => "recovering",
            Self::AlmostRecovered => "almost_recovered",
            Self::Recovered => "recovered",
        }
    }
}

/// 回復の目安時間 (秒)。部位区分ごとの既定値。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryTarget {
    /// 回復中とほぼ回復の境目。
    pub lower_seconds: i64,
    /// ほぼ回復と回復済みの境目。
    pub upper_seconds: i64,
}

impl RecoveryTarget {
    /// 部位区分ごとの既定値。その他は回復状況の対象にしないため None を返す。
    ///
    /// その他を除くすべての部位を 48 〜 72 時間とする (REQ-ANL-009)。
    pub fn of(body_part: BodyPart) -> Option<Self> {
        if body_part.is_other() {
            return None;
        }
        Some(Self {
            lower_seconds: RECOVERY_LOWER_SECONDS,
            upper_seconds: RECOVERY_UPPER_SECONDS,
        })
    }
}

/// 回復状況に使う最後のトレーニングの時刻。
///
/// 終了の時刻がある記録はその時刻を使う。日付だけの記録は、その日の翌日の 0 時
/// (日本時間) を使う。翌日が表現できない日付では None を返す。
pub fn last_training_instant(date: JstDate, finished_at: Option<UtcInstant>) -> Option<UtcInstant> {
    match finished_at {
        Some(finished_at) => Some(finished_at),
        None => Some(date.add_days(1)?.start_utc()),
    }
}

/// 最後のトレーニングの時刻から表示の時点までの経過が、指定の秒数以上か。
///
/// 秒とナノ秒の両方で比べ、境界の手前を境界以上と判定しない。
pub fn elapsed_at_least(now: UtcInstant, last: UtcInstant, seconds: i64) -> bool {
    let elapsed_nanos = (i128::from(now.seconds()) - i128::from(last.seconds())) * NANOS_PER_SECOND
        + i128::from(now.nanos())
        - i128::from(last.nanos());
    elapsed_nanos >= i128::from(seconds) * NANOS_PER_SECOND
}

/// 最後のトレーニングの時刻と表示の時点の時刻から回復の段階を決める。
pub fn recovery_state(now: UtcInstant, last: UtcInstant, target: RecoveryTarget) -> RecoveryState {
    if !elapsed_at_least(now, last, target.lower_seconds) {
        return RecoveryState::Recovering;
    }
    if !elapsed_at_least(now, last, target.upper_seconds) {
        return RecoveryState::AlmostRecovered;
    }
    RecoveryState::Recovered
}

/// 1 つの部位区分の回復状況。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryEntry {
    /// 部位区分。その他は含めない。
    pub body_part: BodyPart,
    /// 最後にトレーニングした時刻。ISO 8601 の UTC の文字列。記録が無ければ None。
    pub last_trained_at: Option<String>,
    /// 回復の段階。最後のトレーニングが無ければ None。
    pub state: Option<RecoveryState>,
}

/// 部位区分ごとの回復状況の応答。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryStatus {
    /// その他を除く 7 区分。`BodyPart::ALL` と同じ並びで、記録の無い部位も含む。
    pub parts: Vec<RecoveryEntry>,
}

/// 部位区分ごとの最後のトレーニングの時刻から回復状況を組み立てる。
///
/// 同じ部位に複数の記録がある場合は、最も新しい時刻を使う。最後のトレーニングが無い
/// 部位は状態を持たず、その他は応答に含めない。
pub fn recovery_status(now: UtcInstant, trainings: &[(BodyPart, UtcInstant)]) -> RecoveryStatus {
    let parts = BodyPart::ALL
        .into_iter()
        .filter_map(|body_part| recovery_entry(now, trainings, body_part))
        .collect();
    RecoveryStatus { parts }
}

fn recovery_entry(
    now: UtcInstant,
    trainings: &[(BodyPart, UtcInstant)],
    body_part: BodyPart,
) -> Option<RecoveryEntry> {
    let target = RecoveryTarget::of(body_part)?;
    let last = trainings
        .iter()
        .filter(|(part, _)| *part == body_part)
        .map(|(_, instant)| *instant)
        .max();
    Some(RecoveryEntry {
        body_part,
        last_trained_at: last.map(UtcInstant::to_iso8601),
        state: last.map(|last| recovery_state(now, last, target)),
    })
}

#[cfg(test)]
mod tests {
    //! 回復状況の境界と、日付だけの記録の時刻の例示テスト。
    //!
    //! 対応する要求: `REQ-ANL-008`、`REQ-ANL-009`。
    //! 任意入力の性質は `backend/pbt/tests/recovery.rs` が担う。ここは 47.9 時間、
    //! 48 時間、71.9 時間、72 時間の境界、最後のトレーニングが無い部位、日付だけの記録、
    //! その他の部位を固定する。

    use super::{
        last_training_instant, recovery_state, recovery_status, RecoveryState, RecoveryStatus,
        RecoveryTarget, RECOVERY_LOWER_SECONDS, RECOVERY_UPPER_SECONDS,
    };
    use crate::exercise::BodyPart;
    use crate::jst::{JstDate, UtcInstant};

    const HOUR: i64 = 60 * 60;

    #[test]
    fn the_state_switches_at_the_48_and_72_hour_boundaries() {
        let last = instant("2026-11-10T00:00:00Z");
        let target = RecoveryTarget::of(BodyPart::Chest).expect("a chest target");
        for (seconds, expected) in [
            (47 * HOUR + 54 * 60, RecoveryState::Recovering),
            (RECOVERY_LOWER_SECONDS, RecoveryState::AlmostRecovered),
            (71 * HOUR + 54 * 60, RecoveryState::AlmostRecovered),
            (RECOVERY_UPPER_SECONDS, RecoveryState::Recovered),
            (80 * HOUR, RecoveryState::Recovered),
        ] {
            let now = UtcInstant::from_epoch_seconds(last.seconds() + seconds);
            assert_eq!(
                recovery_state(now, last, target),
                expected,
                "{seconds} seconds after the last training"
            );
        }
    }

    #[test]
    fn a_boundary_with_a_fractional_second_stays_before_the_next_state() {
        let last = UtcInstant::new(1_000, 900_000_000).expect("a valid instant");
        let target = RecoveryTarget::of(BodyPart::Back).expect("a back target");
        let threshold = last.seconds() + RECOVERY_LOWER_SECONDS;
        let just_before = UtcInstant::new(threshold, 899_999_999).expect("a valid instant");
        let at_boundary = UtcInstant::new(threshold, 900_000_000).expect("a valid instant");
        assert_eq!(
            recovery_state(just_before, last, target),
            RecoveryState::Recovering
        );
        assert_eq!(
            recovery_state(at_boundary, last, target),
            RecoveryState::AlmostRecovered
        );
    }

    #[test]
    fn a_finished_record_uses_its_finish_time() {
        let finished_at = instant("2026-11-12T06:30:00Z");
        assert_eq!(
            last_training_instant(date(2026, 11, 12), Some(finished_at)),
            Some(finished_at)
        );
    }

    #[test]
    fn a_date_only_record_uses_the_next_midnight_in_japan() {
        let last =
            last_training_instant(date(2026, 11, 10), None).expect("a representable instant");
        assert_eq!(last.to_iso8601(), "2026-11-10T15:00:00Z");
        assert_eq!(last.jst_date(), Some(date(2026, 11, 11)));
    }

    #[test]
    fn the_last_representable_date_has_no_next_midnight() {
        assert_eq!(last_training_instant(date(9999, 12, 31), None), None);
    }

    #[test]
    fn the_target_covers_every_part_except_other() {
        for body_part in BodyPart::ALL {
            let target = RecoveryTarget::of(body_part);
            if body_part == BodyPart::Other {
                assert_eq!(target, None, "other is not a target");
                continue;
            }
            assert_eq!(
                target,
                Some(RecoveryTarget {
                    lower_seconds: RECOVERY_LOWER_SECONDS,
                    upper_seconds: RECOVERY_UPPER_SECONDS,
                }),
                "{body_part:?}"
            );
        }
    }

    #[test]
    fn every_part_except_other_has_an_entry_and_the_other_is_excluded() {
        let now = instant("2026-11-12T00:00:00Z");
        let trainings = [
            (BodyPart::Chest, instant("2026-11-11T00:00:00Z")),
            (BodyPart::Other, instant("2026-11-11T23:00:00Z")),
        ];
        let status = recovery_status(now, &trainings);
        assert_eq!(
            status
                .parts
                .iter()
                .map(|part| part.body_part)
                .collect::<Vec<_>>(),
            vec![
                BodyPart::Chest,
                BodyPart::Back,
                BodyPart::Shoulders,
                BodyPart::BicepsForearm,
                BodyPart::Triceps,
                BodyPart::Legs,
                BodyPart::Abs,
            ]
        );
        assert_eq!(
            status
                .parts
                .iter()
                .find(|part| part.body_part == BodyPart::Chest)
                .map(|part| part.state),
            Some(Some(RecoveryState::Recovering))
        );
    }

    #[test]
    fn a_part_without_a_record_has_no_state_and_no_last_time() {
        let now = instant("2026-11-12T00:00:00Z");
        let status = recovery_status(now, &[]);
        for part in &status.parts {
            assert_eq!(part.last_trained_at, None, "{:?}", part.body_part);
            assert_eq!(part.state, None, "{:?}", part.body_part);
        }
    }

    #[test]
    fn the_latest_training_of_a_part_is_used() {
        let now = instant("2026-11-12T00:00:00Z");
        let older = instant("2026-11-07T00:00:00Z");
        let latest = instant("2026-11-11T20:00:00Z");
        let status = recovery_status(now, &[(BodyPart::Legs, older), (BodyPart::Legs, latest)]);
        let legs = status
            .parts
            .iter()
            .find(|part| part.body_part == BodyPart::Legs)
            .expect("the legs entry");
        assert_eq!(
            legs.last_trained_at.as_deref(),
            Some("2026-11-11T20:00:00Z")
        );
        assert_eq!(legs.state, Some(RecoveryState::Recovering));
    }

    #[test]
    fn the_state_values_match_the_api_values() {
        for state in [
            RecoveryState::Recovering,
            RecoveryState::AlmostRecovered,
            RecoveryState::Recovered,
        ] {
            assert_eq!(
                serde_json::to_string(&state).expect("a serializable state"),
                format!("\"{}\"", state.as_str())
            );
        }
    }

    #[test]
    fn the_status_round_trips_through_serde() {
        let status = recovery_status(
            instant("2026-11-12T00:00:00Z"),
            &[(BodyPart::Abs, instant("2026-11-09T00:00:00Z"))],
        );
        let json = serde_json::to_string(&status).expect("a serializable status");
        assert_eq!(
            serde_json::from_str::<RecoveryStatus>(&json).expect("a readable status"),
            status
        );
    }

    fn instant(text: &str) -> UtcInstant {
        UtcInstant::parse(text).expect("a valid instant")
    }

    fn date(year: i64, month: u32, day: u32) -> JstDate {
        JstDate::new(year, month, day).expect("a valid date")
    }
}
