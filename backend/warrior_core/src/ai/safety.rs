//! 痛み、けが、体調不良の記述に対する安全ガード (REQ-AI-016)。

use crate::ai::context::AiContext;
use crate::ai::output::{AiOutput, MenuChangeKind};

/// 負荷を増やす提案をしてはいけない語の一覧。
pub const SAFETY_TERMS: &[&str] = &[
    "痛み",
    "痛い",
    "けが",
    "怪我",
    "故障",
    "体調不良",
    "調子が悪い",
    "めまい",
    "吐き気",
    "しびれ",
];

/// 安全ガードが返す案内。
pub const SAFETY_ADVICE: &str =
    "負荷を下げるか休んでください。記述が続く場合は、医師などの専門家に相談してください。";

/// メモに安全ガードの語が含まれるか。
pub fn memo_has_safety_term(memo: &str) -> bool {
    SAFETY_TERMS.iter().any(|term| memo.contains(term))
}

/// モデルの出力から、痛みやけがのある種目の負荷を増やす提案を除く。
pub fn apply_safety(context: &AiContext, mut output: AiOutput) -> AiOutput {
    let affected = context.safety_exercise_ids();
    if affected.is_empty() {
        return output;
    }

    output
        .proposals
        .retain(|proposal| !affected.contains(&proposal.exercise_id));
    output.menu_changes.retain(|change| {
        !(affected.contains(&change.exercise_id)
            && matches!(
                change.kind,
                MenuChangeKind::SetIncrease | MenuChangeKind::AddExercise
            ))
    });
    output.safety_advice = Some(SAFETY_ADVICE.to_owned());
    output
}

#[cfg(test)]
mod tests {
    use super::{apply_safety, memo_has_safety_term, SAFETY_TERMS};
    use crate::ai::context::{
        AiContext, AiProfile, ContextExercise, ContextTraining, TrainingExperience,
    };
    use crate::ai::output::{AiOutput, MenuChange, MenuChangeKind, SetProposal};
    use crate::jst::JstDate;
    use crate::nutrition::{Goal, Sex};
    use crate::set_calc::RecordMode;

    fn context_with_memo(memo: &str) -> AiContext {
        AiContext {
            goal: Goal::Hypertrophy,
            profile: AiProfile {
                height_cm: 175.0,
                weight_kg: 70.0,
                age: 36,
                sex: Sex::Male,
            },
            experience: TrainingExperience::Intermediate,
            training_days_per_week: 4,
            records: vec![ContextTraining {
                date: JstDate::new(2026, 11, 12).unwrap(),
                memo: None,
                exercises: vec![ContextExercise {
                    exercise_id: "bench-press".to_owned(),
                    name: "ベンチプレス".to_owned(),
                    grip: None,
                    attachment: None,
                    mode: RecordMode::WeightAndReps,
                    memo: Some(memo.to_owned()),
                    sets: Vec::new(),
                }],
            }],
            goals: Vec::new(),
            part_analysis: Vec::new(),
            assisted_set_reference: Vec::new(),
            recovery: Vec::new(),
            routine: Vec::new(),
            exercises: Vec::new(),
        }
    }

    fn output() -> AiOutput {
        AiOutput {
            proposals: vec![SetProposal {
                exercise_id: "bench-press".to_owned(),
                grip: None,
                attachment: None,
                weight_kg: Some(80.0),
                reps: Some(8),
                duration_seconds: None,
                sets: 3,
                reason: "前回よりも重量を増やす".to_owned(),
                reference_reason: None,
            }],
            menu_changes: vec![MenuChange {
                kind: MenuChangeKind::SetIncrease,
                exercise_id: "bench-press".to_owned(),
                target_exercise_id: None,
                sets: Some(4),
                reason: "量を増やす".to_owned(),
            }],
            ..AiOutput::default()
        }
    }

    #[test]
    fn every_safety_term_is_detected() {
        for term in SAFETY_TERMS {
            assert!(memo_has_safety_term(&format!("今日は{term}がある")));
        }
    }

    #[test]
    fn memo_without_a_safety_term_is_not_affected() {
        assert!(!memo_has_safety_term("調子は良い"));
    }

    #[test]
    fn safety_terms_remove_proposals_and_load_increases() {
        let result = apply_safety(&context_with_memo("膝に痛みがある"), output());
        assert!(result.proposals.is_empty());
        assert!(result.menu_changes.is_empty());
        assert_eq!(result.safety_advice.as_deref(), Some(super::SAFETY_ADVICE));
    }

    #[test]
    fn memo_without_a_safety_term_keeps_proposals() {
        let result = apply_safety(&context_with_memo("調子は良い"), output());
        assert_eq!(result.proposals.len(), 1);
        assert_eq!(result.menu_changes.len(), 1);
        assert_eq!(result.safety_advice, None);
    }
}
