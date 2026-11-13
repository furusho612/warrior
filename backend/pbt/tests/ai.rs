//! AI の安全ガードと出力の検証の性質テスト。
//!
//! 対応する要求: `REQ-AI-002`、`REQ-AI-003`、`REQ-AI-013`、`REQ-AI-016`、`REQ-AI-017`。

use proptest::prelude::*;
use warrior_core::ai::context::{
    AiContext, AiProfile, ContextExercise, ContextTraining, PartAnalysis, PartCategory,
    PartSetReference, TrainingExperience,
};
use warrior_core::ai::output::{AiOutput, Evaluation, EvaluationItem, OutputError, SetProposal};
use warrior_core::ai::safety::{apply_safety, SAFETY_ADVICE, SAFETY_TERMS};
use warrior_core::jst::JstDate;
use warrior_core::nutrition::{Goal, Sex};
use warrior_core::set_calc::RecordMode;

fn context(memo: Option<&str>) -> AiContext {
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
                memo: memo.map(str::to_owned),
                sets: Vec::new(),
            }],
        }],
        goals: Vec::new(),
        part_analysis: vec![PartAnalysis {
            part: PartCategory::Chest,
            sets: 10.0,
            total_weight_kg: 1_000.0,
        }],
        assisted_set_reference: vec![PartSetReference {
            part: PartCategory::Chest,
            sets_with_assisted: 10.0,
        }],
        recovery: Vec::new(),
        routine: Vec::new(),
        exercises: Vec::new(),
    }
}

fn evaluation(evidence: &str) -> Evaluation {
    Evaluation {
        good: vec![EvaluationItem {
            text: "重量が伸びた".to_owned(),
            evidence: evidence.to_owned(),
        }],
        improvements: vec![EvaluationItem {
            text: "量を増やす".to_owned(),
            evidence: "胸は週 10 セット".to_owned(),
        }],
        next_actions: vec![EvaluationItem {
            text: "続ける".to_owned(),
            evidence: "前回 70 kg".to_owned(),
        }],
    }
}

fn proposal() -> SetProposal {
    SetProposal {
        exercise_id: "bench-press".to_owned(),
        grip: None,
        attachment: None,
        weight_kg: Some(72.5),
        reps: Some(8),
        duration_seconds: None,
        sets: 3,
        reason: "前回 70 kg から増やす".to_owned(),
    }
}

fn safety_term() -> impl Strategy<Value = &'static str> {
    prop::sample::select(SAFETY_TERMS)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn any_safety_term_removes_the_proposal(term in safety_term()) {
        let context = context(Some(&format!("今日は{term}がある")));
        let output = AiOutput {
            proposals: vec![proposal()],
            ..AiOutput::default()
        };
        let result = apply_safety(&context, output);
        prop_assert!(result.proposals.is_empty());
        prop_assert_eq!(result.safety_advice.as_deref(), Some(SAFETY_ADVICE));
    }

    #[test]
    fn a_memo_without_a_safety_term_keeps_the_proposal(term in "[a-z]{3,10}") {
        let context = context(Some(&format!("調子は良い {term}")));
        let output = AiOutput {
            proposals: vec![proposal()],
            ..AiOutput::default()
        };
        let result = apply_safety(&context, output);
        prop_assert_eq!(result.proposals.len(), 1);
        prop_assert_eq!(result.safety_advice, None);
    }

    #[test]
    fn an_evidence_number_not_in_the_context_is_rejected(number in 1000u32..=9999) {
        let context = context(None);
        let output = AiOutput {
            evaluation: Some(evaluation(&format!("{number} kg を扱えた"))),
            ..AiOutput::default()
        };
        prop_assert_eq!(
            AiOutput::validate(output, &context),
            Err(OutputError::NoDisplayableContent)
        );
    }

    #[test]
    fn an_evidence_number_in_the_context_is_accepted(number in 1u32..=100) {
        let mut context = context(None);
        context.profile.weight_kg = f64::from(number);
        let output = AiOutput {
            evaluation: Some(Evaluation {
                good: vec![EvaluationItem {
                    text: "重量が伸びた".to_owned(),
                    evidence: format!("{number} kg を扱えた"),
                }],
                improvements: vec![EvaluationItem {
                    text: "量を増やす".to_owned(),
                    evidence: "胸は週 10 セット".to_owned(),
                }],
                next_actions: vec![EvaluationItem {
                    text: "続ける".to_owned(),
                    evidence: format!("体重 {number} kg"),
                }],
            }),
            ..AiOutput::default()
        };
        prop_assert!(AiOutput::validate(output, &context).is_ok());
    }

    #[test]
    fn reference_mismatch_requires_a_note(mismatch in 1u32..=10) {
        let mut context = context(None);
        context.assisted_set_reference[0].sets_with_assisted += f64::from(mismatch);
        let output = AiOutput {
            evaluation: Some(evaluation("70 kg を扱えた")),
            ..AiOutput::default()
        };
        prop_assert_eq!(
            AiOutput::validate(output.clone(), &context),
            Err(OutputError::MissingReferenceExplanation)
        );
        let mut output = output;
        output.reference_note = Some("補助筋を含めるとセット数が増える".to_owned());
        prop_assert!(AiOutput::validate(output, &context).is_ok());
    }

    #[test]
    fn proposal_set_counts_outside_one_to_twenty_are_rejected(sets in 21u32..=100) {
        let mut proposal = proposal();
        proposal.sets = sets;
        let output = AiOutput {
            proposals: vec![proposal],
            ..AiOutput::default()
        };
        prop_assert_eq!(
            AiOutput::validate(output, &context(None)),
            Err(OutputError::NoDisplayableContent)
        );
    }

    #[test]
    fn evaluation_item_counts_above_three_are_rejected(extra in 3usize..=6) {
        let mut evaluation = evaluation("70 kg を扱えた");
        for _ in 0..extra {
            evaluation.good.push(EvaluationItem {
                text: "追加".to_owned(),
                evidence: "70 kg".to_owned(),
            });
        }
        let output = AiOutput {
            evaluation: Some(evaluation),
            ..AiOutput::default()
        };
        prop_assert_eq!(
            AiOutput::validate(output, &context(None)),
            Err(OutputError::InvalidEvaluationCount)
        );
    }
}
