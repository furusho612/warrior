//! AI のコンテキスト、プロンプト、出力の検証、安全ガード。

pub mod context;
pub mod output;
pub mod prompt;
pub mod safety;

pub use context::{
    AiContext, AiProfile, ContextExercise, ContextSet, ContextTraining, ExerciseGoal,
    ExerciseSummary, PartAnalysis, PartCategory, PartSetReference, RecoveryPhase, RecoveryStatus,
    RoutineItem, TrainingExperience,
};
pub use output::{
    AiOutput, Evaluation, EvaluationItem, GoalResponse, MenuChange, MenuChangeKind,
    NutritionAdvice, OutputError, SetProposal,
};
pub use prompt::{build_prompt, sanitize_memo, AiPrompt, PROMPT_VERSION};
pub use safety::{apply_safety, memo_has_safety_term, SAFETY_ADVICE, SAFETY_TERMS};
