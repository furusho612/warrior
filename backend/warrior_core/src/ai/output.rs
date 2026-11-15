//! AI の出力の型と検証。

use serde::{Deserialize, Serialize};

use crate::ai::context::{numeric_tokens, AiContext};
use crate::ai::safety::apply_safety;

const MAX_ITEMS_PER_SECTION: usize = 3;
const MAX_TEXT_CHARS: usize = 500;
const MAX_ITEMS: usize = 50;

/// 評価の 1 項目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaluationItem {
    /// 本文。
    pub text: String,
    /// 根拠。記録の数値を含む。
    pub evidence: String,
    /// 補助筋の参考値と画面のセット数が食い違う理由。
    #[serde(default)]
    pub reference_reason: Option<String>,
}

/// 評価。良かった点、改善点、次に取る行動を持つ。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Evaluation {
    /// 良かった点。
    #[serde(default)]
    pub good: Vec<EvaluationItem>,
    /// 改善点。
    #[serde(default)]
    pub improvements: Vec<EvaluationItem>,
    /// 次に取る行動。
    #[serde(default)]
    pub next_actions: Vec<EvaluationItem>,
}

/// 次回セットの提案。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetProposal {
    /// 種目 ID。
    pub exercise_id: String,
    /// グリップ。
    #[serde(default)]
    pub grip: Option<String>,
    /// アタッチメント。
    #[serde(default)]
    pub attachment: Option<String>,
    /// 重量 (kg)。
    #[serde(default)]
    pub weight_kg: Option<f64>,
    /// 回数。
    #[serde(default)]
    pub reps: Option<u32>,
    /// 時間 (秒)。
    #[serde(default)]
    pub duration_seconds: Option<u32>,
    /// セット数。
    pub sets: u32,
    /// 理由 (1 文)。
    pub reason: String,
    /// 補助筋の参考値と画面のセット数が食い違う理由。
    #[serde(default)]
    pub reference_reason: Option<String>,
}

/// メニューの変更の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuChangeKind {
    /// 種目を追加する。
    AddExercise,
    /// 種目を入れ替える。
    ReplaceExercise,
    /// セット数を増やす。
    SetIncrease,
    /// セット数を減らす。
    SetDecrease,
    /// 部位の配分を見直す。
    PartBalance,
}

/// メニューの変更案。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MenuChange {
    /// 変更の種類。
    pub kind: MenuChangeKind,
    /// 種目 ID。
    pub exercise_id: String,
    /// 変更後のセット数。
    #[serde(default)]
    pub sets: Option<u32>,
    /// 理由。
    pub reason: String,
}

/// 目標重量への対応。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalResponse {
    /// 種目 ID。
    pub exercise_id: String,
    /// グリップ。
    #[serde(default)]
    pub grip: Option<String>,
    /// アタッチメント。
    #[serde(default)]
    pub attachment: Option<String>,
    /// 目標を達成できるか。
    pub achievable: bool,
    /// 段階的な途中の目標 (kg)。
    #[serde(default)]
    pub intermediate_targets_kg: Vec<f64>,
    /// 理由。
    pub reason: String,
}

/// 栄養の提案。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NutritionAdvice {
    /// 目標カロリー。
    pub target_kcal: f64,
    /// 週あたりの減量 (kg)。
    #[serde(default)]
    pub weekly_loss_kg: Option<f64>,
}

/// AI の出力。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AiOutput {
    /// 評価。
    #[serde(default)]
    pub evaluation: Option<Evaluation>,
    /// 次回セットの提案。
    #[serde(default)]
    pub proposals: Vec<SetProposal>,
    /// メニューの変更案。
    #[serde(default)]
    pub menu_changes: Vec<MenuChange>,
    /// 目標重量への対応。
    #[serde(default)]
    pub goal_responses: Vec<GoalResponse>,
    /// 栄養の提案。
    #[serde(default)]
    pub nutrition_advice: Option<NutritionAdvice>,
    /// 安全ガードの案内。モデルの出力ではなくサーバーが入れる。
    #[serde(default)]
    pub safety_advice: Option<String>,
}

/// 出力の検証の誤り。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputError {
    /// JSON として読めない。
    InvalidJson(String),
    /// 評価の項目数が範囲外である。
    InvalidEvaluationCount,
    /// 表示できる内容が無い。
    NoDisplayableContent,
    /// 補助筋の参考値との食い違いの理由が無い。
    MissingReferenceExplanation,
    /// 提案の数が多すぎる。
    TooManyProposals,
    /// メニューの変更案が多すぎる。
    TooManyMenuChanges,
}

impl AiOutput {
    /// JSON を読み、検証と安全ガードを通す。
    pub fn from_json(text: &str, context: &AiContext) -> Result<Self, OutputError> {
        let output: AiOutput = serde_json::from_str(text)
            .map_err(|error| OutputError::InvalidJson(error.to_string()))?;
        Self::validate(output, context)
    }

    /// 型付きの出力を検証し、安全ガードを通す。
    pub fn validate(output: AiOutput, context: &AiContext) -> Result<Self, OutputError> {
        let known_numbers = context.known_numbers();
        let requires_reference = context.reference_mismatch();
        let mut output = output;

        if let Some(evaluation) = output.evaluation.take() {
            let evaluation = validate_evaluation(evaluation, &known_numbers, requires_reference)?;
            if evaluation.good.is_empty()
                || evaluation.improvements.is_empty()
                || evaluation.next_actions.is_empty()
            {
                if !requires_reference {
                    return Err(OutputError::NoDisplayableContent);
                }
            } else {
                output.evaluation = Some(evaluation);
            }
        }

        if output.proposals.len() > MAX_ITEMS {
            return Err(OutputError::TooManyProposals);
        }
        output
            .proposals
            .retain(|proposal| valid_proposal(proposal, requires_reference));
        if output.menu_changes.len() > MAX_ITEMS {
            return Err(OutputError::TooManyMenuChanges);
        }
        output.menu_changes.retain(valid_menu_change);
        output.goal_responses = output
            .goal_responses
            .into_iter()
            .filter_map(|response| validate_goal_response(response, context))
            .collect();
        output.nutrition_advice = output
            .nutrition_advice
            .filter(|advice| valid_nutrition_advice(advice, context));

        if requires_reference && !has_reference_explanation(&output) {
            return Err(OutputError::MissingReferenceExplanation);
        }

        let has_content = output.evaluation.is_some()
            || !output.proposals.is_empty()
            || !output.menu_changes.is_empty()
            || !output.goal_responses.is_empty()
            || output.nutrition_advice.is_some()
            || output
                .safety_advice
                .as_deref()
                .is_some_and(|advice| !advice.trim().is_empty());
        if !has_content {
            return Err(OutputError::NoDisplayableContent);
        }

        Ok(apply_safety(context, output))
    }
}

fn validate_evaluation(
    mut evaluation: Evaluation,
    known_numbers: &std::collections::HashSet<String>,
    requires_reference: bool,
) -> Result<Evaluation, OutputError> {
    if evaluation.good.len() > MAX_ITEMS_PER_SECTION
        || evaluation.improvements.len() > MAX_ITEMS_PER_SECTION
        || evaluation.next_actions.len() > MAX_ITEMS_PER_SECTION
    {
        return Err(OutputError::InvalidEvaluationCount);
    }
    evaluation
        .good
        .retain(|item| valid_evaluation_item(item, known_numbers, requires_reference));
    evaluation
        .improvements
        .retain(|item| valid_evaluation_item(item, known_numbers, requires_reference));
    evaluation
        .next_actions
        .retain(|item| valid_evaluation_item(item, known_numbers, requires_reference));
    Ok(evaluation)
}

fn valid_evaluation_item(
    item: &EvaluationItem,
    known_numbers: &std::collections::HashSet<String>,
    requires_reference: bool,
) -> bool {
    if !valid_text(&item.text) || !valid_text(&item.evidence) {
        return false;
    }
    if requires_reference
        && item
            .reference_reason
            .as_deref()
            .is_none_or(|reason| reason.trim().is_empty())
    {
        return false;
    }
    let numbers = numeric_tokens(&item.evidence);
    !numbers.is_empty() && numbers.iter().all(|number| known_numbers.contains(number))
}

fn valid_text(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty() && trimmed.chars().count() <= MAX_TEXT_CHARS
}

fn valid_proposal(proposal: &SetProposal, requires_reference: bool) -> bool {
    if proposal.exercise_id.trim().is_empty()
        || !valid_text(&proposal.reason)
        || !(1..=20).contains(&proposal.sets)
    {
        return false;
    }
    if requires_reference
        && proposal
            .reference_reason
            .as_deref()
            .is_none_or(|reason| reason.trim().is_empty())
    {
        return false;
    }
    if proposal
        .weight_kg
        .is_some_and(|weight| !weight.is_finite() || weight <= 0.0)
    {
        return false;
    }
    if proposal.reps.is_some_and(|reps| !(1..=100).contains(&reps)) {
        return false;
    }
    if proposal
        .duration_seconds
        .is_some_and(|duration| !(1..=86_400).contains(&duration))
    {
        return false;
    }
    if proposal.weight_kg.is_none()
        && proposal.reps.is_none()
        && proposal.duration_seconds.is_none()
    {
        return false;
    }
    true
}

fn has_reference_explanation(output: &AiOutput) -> bool {
    let evaluation_has_reason = output.evaluation.as_ref().is_some_and(|evaluation| {
        evaluation
            .good
            .iter()
            .chain(evaluation.improvements.iter())
            .chain(evaluation.next_actions.iter())
            .any(|item| {
                item.reference_reason
                    .as_deref()
                    .is_some_and(|reason| !reason.trim().is_empty())
            })
    });
    let proposal_has_reason = output.proposals.iter().any(|proposal| {
        proposal
            .reference_reason
            .as_deref()
            .is_some_and(|reason| !reason.trim().is_empty())
    });
    evaluation_has_reason || proposal_has_reason
}

fn valid_menu_change(change: &MenuChange) -> bool {
    !change.exercise_id.trim().is_empty()
        && valid_text(&change.reason)
        && change.sets.is_none_or(|sets| (1..=20).contains(&sets))
}

fn validate_goal_response(response: GoalResponse, context: &AiContext) -> Option<GoalResponse> {
    if !valid_text(&response.reason) {
        return None;
    }
    let goal = context.goals.iter().find(|goal| {
        goal.exercise_id == response.exercise_id
            && goal.grip == response.grip
            && goal.attachment == response.attachment
    })?;
    let mut response = response;
    response.intermediate_targets_kg = crate::goal::valid_intermediate_targets(
        goal.current_weight_kg,
        goal.target_weight_kg,
        &response.intermediate_targets_kg,
    );
    Some(response)
}

fn valid_nutrition_advice(advice: &NutritionAdvice, context: &AiContext) -> bool {
    if !advice.target_kcal.is_finite() || advice.target_kcal <= 0.0 {
        return false;
    }
    if advice.target_kcal < context.profile.basal_metabolism_kcal() {
        return false;
    }
    advice
        .weekly_loss_kg
        .is_none_or(|loss| loss >= 0.0 && loss <= context.profile.weight_kg * 0.01)
}

#[cfg(test)]
mod tests {
    use super::{
        AiOutput, Evaluation, EvaluationItem, GoalResponse, MenuChange, MenuChangeKind,
        NutritionAdvice, OutputError, SetProposal,
    };
    use crate::ai::context::{
        AiContext, AiProfile, ContextExercise, ContextTraining, ExerciseGoal, PartAnalysis,
        PartCategory, PartSetReference, TrainingExperience,
    };
    use crate::jst::JstDate;
    use crate::nutrition::{Goal, Sex};
    use crate::set_calc::RecordMode;

    fn context() -> AiContext {
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
                    memo: None,
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

    #[test]
    fn goal_responses_are_checked_against_the_context_goal() {
        let mut context = context();
        context.goals = vec![ExerciseGoal {
            exercise_id: "bench-press".to_owned(),
            grip: None,
            attachment: None,
            target_weight_kg: 60.0,
            target_date: None,
            current_weight_kg: Some(20.0),
        }];
        let output = AiOutput {
            goal_responses: vec![GoalResponse {
                exercise_id: "bench-press".to_owned(),
                grip: None,
                attachment: None,
                achievable: false,
                intermediate_targets_kg: vec![30.0, 60.0, 20.0, 40.0],
                reason: "段階的に上げる".to_owned(),
            }],
            ..AiOutput::default()
        };
        let result = AiOutput::validate(output, &context).expect("a valid goal response");
        assert_eq!(result.goal_responses.len(), 1);
        assert_eq!(
            result.goal_responses[0].intermediate_targets_kg,
            vec![30.0, 40.0]
        );
    }

    #[test]
    fn a_goal_response_without_a_context_goal_is_removed() {
        let output = AiOutput {
            goal_responses: vec![GoalResponse {
                exercise_id: "unknown".to_owned(),
                grip: None,
                attachment: None,
                achievable: true,
                intermediate_targets_kg: Vec::new(),
                reason: "達成できる".to_owned(),
            }],
            ..AiOutput::default()
        };
        assert_eq!(
            AiOutput::validate(output, &context()),
            Err(OutputError::NoDisplayableContent)
        );
    }

    #[test]
    fn a_goal_response_without_an_intermediate_target_is_valid() {
        let mut context = context();
        context.goals = vec![ExerciseGoal {
            exercise_id: "bench-press".to_owned(),
            grip: None,
            attachment: None,
            target_weight_kg: 60.0,
            target_date: None,
            current_weight_kg: None,
        }];
        let output = AiOutput {
            goal_responses: vec![GoalResponse {
                exercise_id: "bench-press".to_owned(),
                grip: None,
                attachment: None,
                achievable: true,
                intermediate_targets_kg: Vec::new(),
                reason: "達成できる".to_owned(),
            }],
            ..AiOutput::default()
        };
        assert_eq!(
            AiOutput::validate(output, &context)
                .expect("an achievable goal")
                .goal_responses
                .len(),
            1
        );
    }

    fn item(text: &str, evidence: &str) -> EvaluationItem {
        EvaluationItem {
            text: text.to_owned(),
            evidence: evidence.to_owned(),
            reference_reason: None,
        }
    }

    fn evaluation() -> Evaluation {
        Evaluation {
            good: vec![item("重量が伸びた", "70 kg を扱えた")],
            improvements: vec![item("量が少ない", "胸は週 10 セット")],
            next_actions: vec![item("2.5 kg 増やす", "前回 70 kg")],
        }
    }

    fn proposal(reason: &str) -> SetProposal {
        SetProposal {
            exercise_id: "bench-press".to_owned(),
            grip: None,
            attachment: None,
            weight_kg: Some(72.5),
            reps: Some(8),
            duration_seconds: None,
            sets: 3,
            reason: reason.to_owned(),
            reference_reason: None,
        }
    }

    #[test]
    fn valid_output_is_accepted() {
        let output = AiOutput {
            evaluation: Some(evaluation()),
            proposals: vec![proposal("前回 70 kg から増やす")],
            ..AiOutput::default()
        };
        assert!(AiOutput::validate(output, &context()).is_ok());
    }

    #[test]
    fn evidence_without_a_number_is_removed() {
        let mut evaluation = evaluation();
        evaluation.good[0].evidence = "よく頑張った".to_owned();
        let output = AiOutput {
            evaluation: Some(evaluation),
            ..AiOutput::default()
        };
        assert_eq!(
            AiOutput::validate(output, &context()),
            Err(OutputError::NoDisplayableContent)
        );
    }

    #[test]
    fn evidence_with_an_unknown_number_is_removed() {
        let mut evaluation = evaluation();
        evaluation.good[0].evidence = "999 kg を扱えた".to_owned();
        let output = AiOutput {
            evaluation: Some(evaluation),
            ..AiOutput::default()
        };
        assert_eq!(
            AiOutput::validate(output, &context()),
            Err(OutputError::NoDisplayableContent)
        );
    }

    #[test]
    fn reference_mismatch_requires_a_reason_on_an_item() {
        let mut context = context();
        context.assisted_set_reference[0].sets_with_assisted = 12.0;
        let output = AiOutput {
            proposals: vec![proposal("前回 70 kg から増やす")],
            ..AiOutput::default()
        };
        assert_eq!(
            AiOutput::validate(output, &context),
            Err(OutputError::MissingReferenceExplanation)
        );

        let mut proposal = proposal("前回 70 kg から増やす");
        proposal.reference_reason = Some("補助筋を含めると 12 セットになる".to_owned());
        let output = AiOutput {
            proposals: vec![proposal],
            ..AiOutput::default()
        };
        assert!(AiOutput::validate(output, &context).is_ok());
    }

    #[test]
    fn an_evaluation_without_reasons_is_dropped_when_proposals_explain_the_reference() {
        let mut context = context();
        context.assisted_set_reference[0].sets_with_assisted = 12.0;
        let mut proposal = proposal("前回 70 kg から増やす");
        proposal.reference_reason = Some("補助筋を含めると 12 セットになる".to_owned());
        let output = AiOutput {
            evaluation: Some(evaluation()),
            proposals: vec![proposal],
            ..AiOutput::default()
        };
        let result = AiOutput::validate(output, &context).unwrap();
        assert_eq!(result.evaluation, None);
        assert_eq!(result.proposals.len(), 1);
    }

    #[test]
    fn nutrition_advice_below_bmr_is_removed() {
        let output = AiOutput {
            nutrition_advice: Some(NutritionAdvice {
                target_kcal: 1.0,
                weekly_loss_kg: None,
            }),
            ..AiOutput::default()
        };
        assert_eq!(
            AiOutput::validate(output, &context()),
            Err(OutputError::NoDisplayableContent)
        );
    }

    #[test]
    fn weekly_loss_over_one_percent_is_removed() {
        let output = AiOutput {
            nutrition_advice: Some(NutritionAdvice {
                target_kcal: 2_500.0,
                weekly_loss_kg: Some(1.0),
            }),
            ..AiOutput::default()
        };
        assert_eq!(
            AiOutput::validate(output, &context()),
            Err(OutputError::NoDisplayableContent)
        );
    }

    #[test]
    fn empty_or_invalid_proposals_are_removed() {
        let mut empty_reason = proposal("");
        empty_reason.weight_kg = Some(80.0);
        let mut zero_sets = proposal("理由");
        zero_sets.sets = 0;
        let mut negative_weight = proposal("理由");
        negative_weight.weight_kg = Some(-1.0);
        let output = AiOutput {
            proposals: vec![empty_reason, zero_sets, negative_weight],
            ..AiOutput::default()
        };
        assert_eq!(
            AiOutput::validate(output, &context()),
            Err(OutputError::NoDisplayableContent)
        );
    }

    #[test]
    fn too_many_evaluation_items_are_rejected() {
        let mut evaluation = evaluation();
        evaluation.good = vec![
            item("a", "70 kg"),
            item("b", "70 kg"),
            item("c", "70 kg"),
            item("d", "70 kg"),
        ];
        let output = AiOutput {
            evaluation: Some(evaluation),
            ..AiOutput::default()
        };
        assert_eq!(
            AiOutput::validate(output, &context()),
            Err(OutputError::InvalidEvaluationCount)
        );
    }

    #[test]
    fn too_long_text_is_removed() {
        let mut evaluation = evaluation();
        evaluation.good[0].text = "あ".repeat(501);
        let output = AiOutput {
            evaluation: Some(evaluation),
            ..AiOutput::default()
        };
        assert_eq!(
            AiOutput::validate(output, &context()),
            Err(OutputError::NoDisplayableContent)
        );
    }

    #[test]
    fn menu_changes_are_validated() {
        let output = AiOutput {
            menu_changes: vec![MenuChange {
                kind: MenuChangeKind::SetIncrease,
                exercise_id: "bench-press".to_owned(),
                sets: Some(4),
                reason: "胸が週 10 セット".to_owned(),
            }],
            ..AiOutput::default()
        };
        assert!(AiOutput::validate(output, &context()).is_ok());
    }

    #[test]
    fn safety_terms_are_applied_after_validation() {
        let mut context = context();
        context.records[0].exercises[0].memo = Some("膝に痛みがある".to_owned());
        let output = AiOutput {
            proposals: vec![proposal("前回 70 kg から増やす")],
            ..AiOutput::default()
        };
        let result = AiOutput::validate(output, &context).unwrap();
        assert!(result.proposals.is_empty());
        assert!(result.safety_advice.is_some());
    }
}
