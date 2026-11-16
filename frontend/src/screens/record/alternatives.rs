//! 記録中の種目の代わりの候補の表示と入れ替え。
//!
//! 候補は端末内に保存した種目ライブラリと、過去に記録した種目の ID から選ぶ。
//! 通信できない状態でも同じ候補を表示する。候補を選ぶと、種目の行の ID と位置、
//! メモを保ったまま、スナップショットと種目の ID を選んだ種目へ置き換える。
//! 記録済みのセットがあるときは、入れ替える前に確認する (REQ-ALT-001、REQ-ALT-006)。

use std::collections::HashSet;

use dioxus::prelude::*;
use warrior_core::alternative::{select_alternatives, AlternativeExercise, AlternativeOrigin};
use warrior_core::exercise::{Exercise, ExerciseSnapshot};
use warrior_core::training_record::TrainingExerciseRecord;

use crate::storage;
use crate::sync;
use crate::text::{text, Key};
use crate::training_exercise;

/// 種目の操作に置く、代わりの種目の候補の表示と入れ替え。
#[component]
pub(super) fn AlternativePanel(
    record: TrainingExerciseRecord,
    snapshot: ExerciseSnapshot,
    disabled: bool,
    has_completed: bool,
    on_changed: EventHandler<()>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut loading = use_signal(|| false);
    let mut candidates = use_signal(Vec::<AlternativeExercise>::new);
    let mut exercises = use_signal(Vec::<Exercise>::new);
    let mut pending = use_signal(|| Option::<AlternativeExercise>::None);
    let busy = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);

    let open_snapshot = snapshot.clone();
    let open_panel = EventHandler::new(move |_: ()| {
        open.set(true);
        loading.set(true);
        error.set(None);
        let snapshot = open_snapshot.clone();
        spawn(async move {
            let result = match storage::current().await {
                Ok(storage) => storage.alternative_source().await,
                Err(error) => Err(error),
            };
            match result {
                Ok((library, recorded)) => {
                    let recorded = recorded.into_iter().collect::<HashSet<_>>();
                    match library
                        .iter()
                        .find(|exercise| exercise.id == snapshot.exercise_id)
                    {
                        Some(origin) => {
                            candidates.set(select_alternatives(
                                &AlternativeOrigin::from(origin),
                                &library,
                                &recorded,
                            ));
                            exercises.set(library);
                        }
                        None => {
                            candidates.set(Vec::new());
                            exercises.set(library);
                        }
                    }
                }
                Err(_) => {
                    error.set(Some(text(Key::RecordAlternativeLoadFailed).to_owned()));
                }
            }
            loading.set(false);
        });
    });

    let replace_record = record.clone();
    let start_replace = EventHandler::new(move |exercise: Exercise| {
        spawn(replace(
            replace_record.clone(),
            exercise,
            busy,
            error,
            on_changed,
        ));
    });

    let choose = EventHandler::new(move |candidate: AlternativeExercise| {
        let Some(exercise) = exercises
            .peek()
            .iter()
            .find(|exercise| exercise.id == candidate.exercise_id)
            .cloned()
        else {
            error.set(Some(text(Key::RecordAlternativeLoadFailed).to_owned()));
            return;
        };
        if has_completed {
            pending.set(Some(candidate));
            return;
        }
        start_replace.call(exercise);
    });

    let confirm = EventHandler::new(move |_: ()| {
        let Some(candidate) = (pending)() else {
            return;
        };
        let Some(exercise) = exercises
            .peek()
            .iter()
            .find(|exercise| exercise.id == candidate.exercise_id)
            .cloned()
        else {
            error.set(Some(text(Key::RecordAlternativeLoadFailed).to_owned()));
            return;
        };
        pending.set(None);
        start_replace.call(exercise);
    });

    let error_message = (error)();
    let pending_candidate = (pending)();
    let busy_now = (busy)();
    let loading_now = (loading)();
    let list = (candidates)();

    rsx! {
        div {
            class: "flex flex-col gap-2",
            "data-alternatives-panel": "true",
            button {
                class: "min-h-11 self-start border border-line px-2 text-[11px]",
                "data-alternatives-open": "true",
                disabled: disabled,
                onclick: move |_| open_panel.call(()),
                "{text(Key::RecordAlternativeOpen)}"
            }
            if open() {
                div { class: "flex flex-col gap-1 border border-line bg-surface p-2",
                    div { class: "flex items-center justify-between gap-2",
                        h4 { class: "text-[11px] font-bold", "{text(Key::RecordAlternativeTitle)}" }
                        button {
                            class: "min-h-11 border border-line px-2 text-[11px]",
                            "data-alternatives-close": "true",
                            onclick: move |_| open.set(false),
                            "{text(Key::RecordAlternativeClose)}"
                        }
                    }
                    if loading_now {
                        p { class: "text-[11px] text-text-sub", "{text(Key::RecordLoading)}" }
                    } else if list.is_empty() {
                        p {
                            class: "text-[11px] text-text-sub",
                            "data-alternatives-empty": "true",
                            "{text(Key::RecordAlternativeEmpty)}"
                        }
                    } else {
                        div { class: "flex flex-col gap-1", "data-alternatives": "true",
                            for candidate in list {
                                {
                                    let candidate_id = candidate.exercise_id.clone();
                                    let candidate_for_click = candidate.clone();
                                    rsx! {
                                        button {
                                            class: "flex min-h-11 flex-col items-start border border-line px-2 text-left text-[11px]",
                                            "data-alternative": "true",
                                            "data-exercise-id": "{candidate_id}",
                                            disabled: busy_now,
                                            onclick: move |_| choose.call(candidate_for_click.clone()),
                                            span { class: "font-bold", "{candidate.name}" }
                                            span {
                                                class: "text-text-sub",
                                                "{text(Key::RecordAlternativeOverlap)}: {candidate.shared_primary_muscles}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(candidate) = pending_candidate {
                        div {
                            class: "flex flex-col gap-2 border border-primary p-2",
                            "data-alternative-confirmation": "true",
                            p { class: "text-[11px] leading-5", "{text(Key::RecordAlternativeConfirm)}" }
                            div { class: "flex gap-2",
                                button {
                                    class: "min-h-11 border border-primary bg-primary px-2 text-[11px] font-bold text-on-primary",
                                    "data-alternative-confirm": "true",
                                    disabled: busy_now,
                                    onclick: move |_| confirm.call(()),
                                    "{text(Key::RecordAlternativeConfirmAction)}"
                                }
                                button {
                                    class: "min-h-11 border border-line px-2 text-[11px]",
                                    "data-alternative-cancel": "true",
                                    disabled: busy_now,
                                    onclick: move |_| pending.set(None),
                                    "{text(Key::RecordAlternativeCancel)}"
                                }
                            }
                            p { class: "text-[11px] text-text-sub", "{candidate.name}" }
                        }
                    }
                }
            }
            if let Some(message) = error_message {
                p {
                    class: "text-[11px] text-primary",
                    "data-alternative-error": "true",
                    "{message}"
                }
            }
        }
    }
}

async fn replace(
    record: TrainingExerciseRecord,
    exercise: Exercise,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
    on_changed: EventHandler<()>,
) {
    busy.set(true);
    let result = match training_exercise::replace(&record, &exercise) {
        Ok((updated, operation)) => match storage::current().await {
            Ok(storage) => {
                storage
                    .save_training_exercise_create(&updated, &operation)
                    .await
            }
            Err(error) => Err(error),
        },
        Err(error) => Err(error),
    };
    busy.set(false);
    match result {
        Ok(()) => {
            error.set(None);
            sync::flush_in_background();
            on_changed.call(());
        }
        Err(_) => error.set(Some(text(Key::RecordAlternativeReplaceFailed).to_owned())),
    }
}
