//! 記録画面の下部に固定するレストタイマーと、レスト時間の設定 (REQ-TMR-001 から REQ-TMR-006)。
//!
//! 残り時間は保存した終了予定時刻と現在時刻の差から求め、一定間隔の処理は
//! 画面の再描画にだけ使う (ADR-007)。延長と短縮は終了予定時刻を変え、スキップは
//! タイマーを終了する。終了の音と振動、ブラウザ通知は `crate::notification`、
//! 画面の消灯防止は `crate::wake_lock` が担う。

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use warrior_core::notification::{PageVisibility, Permission};
use warrior_core::rest_timer::{self as core_timer, RestTimer, DEFAULT_SECONDS, STEP_SECONDS};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

use super::set_events::SetCompletedEvents;
use super::sleep_ms;
use crate::notification;
use crate::push::PushSubscriptionSetting;
use crate::rest_timer::{now_ms, ActiveRestTimer, RestTimerState};
use crate::storage;
use crate::text::{text, Key};
use crate::wake_lock::ScreenWakeLock;

/// 残り時間を画面へ反映する間隔 (ミリ秒)。
const TICK_INTERVAL_MS: i32 = 250;

/// 表示に戻ったときに再描画する listener。
type VisibilityClosure = Closure<dyn FnMut(web_sys::Event)>;
/// listener を画面の寿命の間だけ保持する。
type VisibilityListener = Rc<RefCell<Option<VisibilityClosure>>>;

/// 記録画面の下部に固定するレストタイマー。
#[component]
pub(crate) fn RestTimerBar(training_id: Option<String>) -> Element {
    let state = use_context::<RestTimerState>();
    let tick = use_signal(|| 0u64);
    let loaded = use_signal(|| false);
    let mut notified_end = use_signal(|| Option::<i64>::None);
    let mut seen_running = use_signal(|| false);
    let mut sound_blocked = use_signal(|| false);
    let listener = use_hook(|| Rc::new(RefCell::new(None::<VisibilityClosure>)));
    let install = listener.clone();
    use_effect(move || install_visibility_tick(install.clone(), tick));
    let remove = listener;
    use_drop(move || remove_visibility_tick(&remove));
    let load_id = training_id.clone();
    use_effect(move || {
        let id = load_id.clone();
        spawn(load_timer(id, state, loaded));
    });
    use_future(move || async move {
        let mut tick = tick;
        loop {
            sleep_ms(TICK_INTERVAL_MS).await;
            let next = (tick)().wrapping_add(1);
            tick.set(next);
        }
    });
    let _ = (tick)();

    let training = training_id.clone();
    let active = training
        .as_deref()
        .and_then(|id| (state.timer)().filter(|active| active.training_id == id));
    let remaining = active
        .as_ref()
        .map(|active| active.timer.remaining_seconds(now_ms()));
    let finished = remaining == Some(0);
    let end_ms = active.as_ref().map(|active| active.timer.end_ms());
    let notification_enabled = (state.notification_enabled)();
    use_effect(use_reactive(
        (&finished, &end_ms, &notification_enabled),
        move |(finished, end_ms, notification_enabled)| {
            if finished {
                let Some(end_ms) = end_ms else {
                    return;
                };
                if *notified_end.peek() == Some(end_ms) {
                    return;
                }
                if *seen_running.peek()
                    && !notification::notify_rest_finished(notification_enabled, page_visibility())
                {
                    sound_blocked.set(true);
                }
                notified_end.set(Some(end_ms));
                return;
            }
            match end_ms {
                Some(end_ms) => {
                    if *notified_end.peek() == Some(end_ms) {
                        notified_end.set(None);
                    }
                    seen_running.set(true);
                }
                None => {
                    notified_end.set(None);
                    seen_running.set(false);
                }
            }
        },
    ));

    let Some(training_id) = training_id else {
        return idle_timer(false);
    };
    let Some(active) = active else {
        return idle_timer(loaded());
    };
    render_timer(
        training_id,
        active,
        remaining.unwrap_or(0),
        finished,
        sound_blocked,
        state,
    )
}

/// セットの完了のイベントを受け取り、レストタイマーを開始する。
#[component]
pub(crate) fn RestTimerEvents() -> Element {
    let events = use_context::<SetCompletedEvents>();
    let state = use_context::<RestTimerState>();
    let mut processed = use_signal(|| 0usize);
    use_effect(move || {
        let count = events.count();
        let processed_count = *processed.peek();
        if count <= processed_count {
            return;
        }
        let completed = events.0.peek().clone();
        processed.set(count);
        for event in completed
            .into_iter()
            .skip(processed_count)
            .filter(|event| event.starts_rest_timer)
        {
            spawn(start_after_set_completed(
                event.training_id,
                event.training_exercise_id,
                event.set_id,
                state,
            ));
        }
    });
    rsx! {}
}

fn install_visibility_tick(listener: VisibilityListener, mut tick: Signal<u64>) {
    if listener.borrow().is_some() {
        return;
    }
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let closure = Closure::<dyn FnMut(web_sys::Event)>::new(move |_event: web_sys::Event| {
        if !document_is_visible() {
            return;
        }
        let next = (tick)().wrapping_add(1);
        tick.set(next);
    });
    if document
        .add_event_listener_with_callback("visibilitychange", closure.as_ref().unchecked_ref())
        .is_ok()
    {
        *listener.borrow_mut() = Some(closure);
    }
}

fn remove_visibility_tick(listener: &VisibilityListener) {
    let closure = listener.borrow_mut().take();
    let (Some(closure), Some(document)) = (
        closure,
        web_sys::window().and_then(|window| window.document()),
    ) else {
        return;
    };
    // 画面を離れるときの後始末であり、削除の失敗で記録を止めない。
    let _ = document
        .remove_event_listener_with_callback("visibilitychange", closure.as_ref().unchecked_ref());
}

fn document_is_visible() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_some_and(|document| document.visibility_state() == web_sys::VisibilityState::Visible)
}

fn page_visibility() -> PageVisibility {
    if document_is_visible() {
        PageVisibility::Visible
    } else {
        PageVisibility::Hidden
    }
}

fn idle_timer(loaded: bool) -> Element {
    rsx! {
        div {
            class: "flex w-full items-center justify-between px-4",
            "data-rest-timer": "true",
            "data-rest-timer-active": "false",
            "data-rest-timer-loaded": if loaded { "true" } else { "false" },
            span { class: "font-bold", "{text(Key::RestTimer)}" }
            span { class: "text-[10px]", "{text(Key::RestTimerIdle)}" }
        }
    }
}

fn render_timer(
    training_id: String,
    active: ActiveRestTimer,
    remaining: u64,
    finished: bool,
    sound_blocked: Signal<bool>,
    state: RestTimerState,
) -> Element {
    let seconds = u32::try_from(remaining).unwrap_or(u32::MAX);
    let remaining_text = core_timer::format(seconds);
    let end_ms = active.timer.end_ms();
    let extend_id = training_id.clone();
    let shorten_id = training_id.clone();
    let skip_id = training_id.clone();
    let extend_timer = active.timer;
    let shorten_timer = active.timer;
    rsx! {
        div {
            class: "flex w-full flex-col gap-1 px-3 py-1",
            "data-rest-timer": "true",
            "data-rest-timer-active": "true",
            "data-rest-timer-remaining-seconds": "{remaining}",
            "data-rest-timer-end-ms": "{end_ms}",
            "data-rest-timer-finished": if finished { "true" } else { "false" },
            div { class: "flex items-center justify-between gap-2",
                span { class: "text-[11px] font-bold", "{text(Key::RestTimer)}" }
                span {
                    class: "font-condensed text-[20px] font-bold tabular-nums",
                    "data-rest-timer-remaining": "{remaining_text}",
                    if finished {
                        "{text(Key::RestTimerFinished)}"
                    } else {
                        "{remaining_text}"
                    }
                }
                div { class: "flex items-center gap-1",
                    button {
                        class: "min-h-11 border border-line px-2 text-[11px]",
                        "data-rest-timer-extend": "true",
                        onclick: move |_| {
                            let timer = extend_timer.extended(STEP_SECONDS);
                            state.replace(&extend_id, timer);
                            spawn(persist_timer(extend_id.clone(), timer));
                        },
                        "{text(Key::RestTimerExtend)}"
                    }
                    button {
                        class: "min-h-11 border border-line px-2 text-[11px]",
                        "data-rest-timer-shorten": "true",
                        onclick: move |_| {
                            let timer = shorten_timer.shortened(STEP_SECONDS);
                            state.replace(&shorten_id, timer);
                            spawn(persist_timer(shorten_id.clone(), timer));
                        },
                        "{text(Key::RestTimerShorten)}"
                    }
                    button {
                        class: "min-h-11 border border-line px-2 text-[11px]",
                        "data-rest-timer-skip": "true",
                        onclick: move |_| {
                            state.clear();
                            spawn(clear_timer(skip_id.clone()));
                        },
                        if finished {
                            "{text(Key::RestTimerClose)}"
                        } else {
                            "{text(Key::RestTimerSkip)}"
                        }
                    }
                }
            }
            if sound_blocked() {
                p {
                    class: "text-[10px] text-primary",
                    "data-rest-timer-sound-blocked": "true",
                    "{text(Key::RestTimerSoundBlocked)}"
                }
            }
        }
    }
}

async fn load_timer(training_id: Option<String>, state: RestTimerState, mut loaded: Signal<bool>) {
    let Some(training_id) = training_id else {
        loaded.set(true);
        return;
    };
    let Ok(storage) = storage::current().await else {
        loaded.set(true);
        return;
    };
    if let Ok(Some(end_ms)) = storage.rest_timer_end(&training_id).await {
        state.start(&training_id, RestTimer::from_end_ms(end_ms));
    }
    loaded.set(true);
}

async fn persist_timer(training_id: String, timer: RestTimer) {
    let Ok(storage) = storage::current().await else {
        return;
    };
    let _ = storage.save_rest_timer(&training_id, timer.end_ms()).await;
}

async fn clear_timer(training_id: String) {
    let Ok(storage) = storage::current().await else {
        return;
    };
    let _ = storage.clear_rest_timer(&training_id).await;
}

/// レスト時間の設定。画面の消灯防止と、全体の既定値を持つ。
#[component]
pub(crate) fn RestSettingsPanel() -> Element {
    rsx! {
        div {
            class: "flex flex-col gap-2 border border-line bg-surface p-3",
            "data-rest-settings": "true",
            RestSecondsField {
                target: RestSecondsTarget::Default,
                disabled: false,
            }
            WakeLockSetting {}
            RestNotificationSetting {}
            PushSubscriptionSetting {}
        }
    }
}

/// 画面の消灯防止を有効にするかの設定。
#[component]
pub(crate) fn WakeLockSetting() -> Element {
    let state = use_context::<RestTimerState>();
    let mut enabled = state.wake_lock_enabled;
    let error = use_signal(|| Option::<String>::None);
    use_effect(move || {
        spawn(async move {
            let Ok(storage) = storage::current().await else {
                return;
            };
            if let Ok(value) = storage.wake_lock_enabled().await {
                enabled.set(value);
            }
        });
    });
    let enabled_value = enabled();
    rsx! {
        div { class: "flex flex-col gap-1", "data-wake-lock-setting": "true",
            label {
                class: "flex min-h-11 items-center gap-2 text-[13px]",
                input {
                    r#type: "checkbox",
                    class: "h-5 w-5",
                    "data-wake-lock": "true",
                    checked: enabled_value,
                    onchange: move |event| {
                        let next = event.checked();
                        enabled.set(next);
                        spawn(save_wake_lock(next, error));
                    },
                }
                "{text(Key::WakeLockLabel)}"
            }
            if let Some(message) = error() {
                p { class: "text-[11px] text-primary", "{message}" }
            }
        }
    }
}

/// レストの終了をブラウザ通知で知らせる設定。
///
/// 通知の許可は、利用者がこの設定を有効にしたときにだけ求める。画面の読み込み時には求めない。
#[component]
pub(crate) fn RestNotificationSetting() -> Element {
    let state = use_context::<RestTimerState>();
    let mut enabled = state.notification_enabled;
    let message = use_signal(|| Option::<String>::None);
    use_effect(move || {
        spawn(async move {
            let Ok(storage) = storage::current().await else {
                return;
            };
            if let Ok(value) = storage.browser_notification_enabled().await {
                enabled.set(value);
            }
        });
    });
    let enabled_value = enabled();
    rsx! {
        div {
            class: "flex flex-col gap-1",
            "data-notification-enabled": if enabled_value { "true" } else { "false" },
            label {
                class: "flex min-h-11 items-center gap-2 text-[13px]",
                input {
                    r#type: "checkbox",
                    class: "h-5 w-5",
                    "data-notification": "true",
                    checked: enabled_value,
                    onchange: move |event| {
                        let next = event.checked();
                        let previous = enabled();
                        enabled.set(next);
                        spawn(save_notification_setting(previous, next, enabled, message));
                    },
                }
                "{text(Key::RestNotificationLabel)}"
            }
            if let Some(value) = message() {
                p {
                    class: "text-[11px] text-primary",
                    "data-notification-message": "true",
                    "{value}"
                }
            }
        }
    }
}

async fn save_notification_setting(
    previous: bool,
    next: bool,
    mut enabled: Signal<bool>,
    mut message: Signal<Option<String>>,
) {
    message.set(None);
    let saved = match storage::current().await {
        Ok(storage) => {
            if notification::should_request_permission(previous, next) {
                match notification::request_permission().await {
                    Some(Permission::Granted) => {}
                    Some(Permission::Denied) => {
                        enabled.set(false);
                        let _ = storage.save_browser_notification_enabled(false).await;
                        message.set(Some(text(Key::RestNotificationDenied).to_owned()));
                        return;
                    }
                    Some(Permission::Default) => {
                        enabled.set(false);
                        let _ = storage.save_browser_notification_enabled(false).await;
                        message.set(Some(text(Key::RestNotificationPending).to_owned()));
                        return;
                    }
                    None => {
                        enabled.set(false);
                        let _ = storage.save_browser_notification_enabled(false).await;
                        message.set(Some(text(Key::RestNotificationUnsupported).to_owned()));
                        return;
                    }
                }
            }
            storage.save_browser_notification_enabled(next).await
        }
        Err(_) => Err(storage::StorageError::Unavailable(
            "the storage is missing".to_owned(),
        )),
    };
    if saved.is_err() {
        enabled.set(false);
        message.set(Some(text(Key::NotificationSaveFailed).to_owned()));
    }
}

/// 画面の消灯防止の設定を有効にするか。
#[component]
pub(crate) fn RecordWakeLock(training_id: Option<String>) -> Element {
    let state = use_context::<RestTimerState>();
    let controller = use_hook(|| Rc::new(ScreenWakeLock::default()));
    let enabled = (state.wake_lock_enabled)();
    let active = training_id.is_some();
    let start_controller = controller.clone();
    use_effect(use_reactive(
        (&active, &enabled),
        move |(active, enabled)| {
            if active && enabled {
                start_controller.start();
            } else {
                start_controller.stop();
            }
        },
    ));
    let drop_controller = controller;
    use_drop(move || drop_controller.stop());
    rsx! {}
}

async fn save_wake_lock(enabled: bool, mut error: Signal<Option<String>>) {
    let storage = match storage::current().await {
        Ok(storage) => storage,
        Err(_) => {
            error.set(Some(text(Key::WakeLockSaveFailed).to_owned()));
            return;
        }
    };
    match storage.save_wake_lock_enabled(enabled).await {
        Ok(()) => error.set(None),
        Err(_) => error.set(Some(text(Key::WakeLockSaveFailed).to_owned())),
    }
}

/// レスト時間を設定する対象。
#[derive(Clone, PartialEq, Eq)]
pub(super) enum RestSecondsTarget {
    /// 全体の既定値。
    Default,
    /// 種目ごとの値。
    Exercise(String),
    /// セットごとの値。
    Set(String),
}

impl RestSecondsTarget {
    fn kind(&self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Exercise(_) => "exercise",
            Self::Set(_) => "set",
        }
    }

    fn id(&self) -> &str {
        match self {
            Self::Default => "",
            Self::Exercise(id) | Self::Set(id) => id,
        }
    }

    fn label(&self) -> Key {
        match self {
            Self::Default => Key::RestTimerDefault,
            Self::Exercise(_) => Key::RestTimerExercise,
            Self::Set(_) => Key::RestTimerSet,
        }
    }

    fn is_default(&self) -> bool {
        matches!(self, Self::Default)
    }

    fn is_exercise(&self) -> bool {
        matches!(self, Self::Exercise(_))
    }
}

/// レスト時間の入力。空にすると指定を消し、既定値または上位の指定を使う。
#[component]
pub(super) fn RestSecondsField(target: RestSecondsTarget, disabled: bool) -> Element {
    let mut value = use_signal(String::new);
    let error = use_signal(|| Option::<String>::None);
    let load_target = target.clone();
    use_effect(move || {
        spawn(load_seconds(load_target.clone(), value, error));
    });
    let input_target = target.clone();
    let kind = target.kind();
    let label = target.label();
    let is_default = target.is_default();
    let is_exercise = target.is_exercise();
    let target_id = target.id().to_owned();
    let current = value();
    let placeholder = if is_default {
        DEFAULT_SECONDS.to_string()
    } else {
        text(Key::RestTimerOverridePlaceholder).to_owned()
    };
    rsx! {
        div { class: "flex flex-col gap-1",
            label { class: "flex items-center justify-between gap-2 text-[13px]",
                span { "{text(label)}" }
                input {
                    class: "min-h-11 w-24 border border-line bg-bg px-2 text-[13px] tabular-nums",
                    r#type: "text",
                    inputmode: "numeric",
                    "data-rest-seconds": "true",
                    "data-rest-target": "{kind}",
                    "data-rest-target-id": "{target_id}",
                    "data-rest-default": if is_default { "true" } else { "false" },
                    "data-rest-exercise": if is_exercise { target_id.clone() } else { String::new() },
                    "data-rest-set": if kind == "set" { target_id.clone() } else { String::new() },
                    value: "{current}",
                    placeholder: "{placeholder}",
                    disabled,
                    oninput: move |event| {
                        let text_value = event.value();
                        value.set(text_value.clone());
                        spawn(save_seconds(input_target.clone(), text_value, error));
                    },
                }
            }
            if let Some(message) = error() {
                p {
                    class: "text-[11px] text-primary",
                    "data-rest-seconds-error": "true",
                    "{message}"
                }
            }
        }
    }
}

async fn load_seconds(
    target: RestSecondsTarget,
    mut value: Signal<String>,
    mut error: Signal<Option<String>>,
) {
    let storage = match storage::current().await {
        Ok(storage) => storage,
        Err(_) => {
            error.set(Some(text(Key::RestTimerSaveFailed).to_owned()));
            return;
        }
    };
    match load_target_seconds(&storage, &target).await {
        Ok(seconds) => {
            value.set(seconds.map_or_else(String::new, |seconds| seconds.to_string()));
            error.set(None);
        }
        Err(_) => error.set(Some(text(Key::RestTimerSaveFailed).to_owned())),
    }
}

async fn save_seconds(
    target: RestSecondsTarget,
    text_value: String,
    mut error: Signal<Option<String>>,
) {
    let Ok(seconds) = parse_seconds(&text_value) else {
        error.set(Some(text(Key::RestTimerInvalid).to_owned()));
        return;
    };
    let storage = match storage::current().await {
        Ok(storage) => storage,
        Err(_) => {
            error.set(Some(text(Key::RestTimerSaveFailed).to_owned()));
            return;
        }
    };
    if save_target_seconds(&storage, &target, seconds)
        .await
        .is_err()
    {
        error.set(Some(text(Key::RestTimerSaveFailed).to_owned()));
        return;
    }
    error.set(None);
}

fn parse_seconds(text_value: &str) -> Result<Option<u32>, ()> {
    let text_value = text_value.trim();
    if text_value.is_empty() {
        return Ok(None);
    }
    core_timer::parse(text_value).map(Some).ok_or(())
}

async fn load_target_seconds(
    storage: &storage::Storage,
    target: &RestSecondsTarget,
) -> Result<Option<u32>, storage::StorageError> {
    match target {
        RestSecondsTarget::Default => Ok(Some(crate::settings::rest_default_seconds().await)),
        RestSecondsTarget::Exercise(id) => storage.rest_exercise_seconds(id).await,
        RestSecondsTarget::Set(id) => storage.rest_set_seconds(id).await,
    }
}

async fn save_target_seconds(
    storage: &storage::Storage,
    target: &RestSecondsTarget,
    seconds: Option<u32>,
) -> Result<(), storage::StorageError> {
    match target {
        RestSecondsTarget::Default => {
            crate::settings::set_rest_default_seconds(seconds.unwrap_or(DEFAULT_SECONDS))
                .await
                .map(|_| ())
                .map_err(|_| {
                    storage::StorageError::InvalidOperation(
                        "the settings could not be saved".to_owned(),
                    )
                })
        }
        RestSecondsTarget::Exercise(id) => storage.save_rest_exercise_seconds(id, seconds).await,
        RestSecondsTarget::Set(id) => storage.save_rest_set_seconds(id, seconds).await,
    }
}

/// セットの完了のイベントから、レストタイマーを開始する。
///
/// セットの指定、種目の指定、全体の既定値の順に優先する。完了の書き込みが終わった
/// 後に呼ぶ。タイマーはトレーニングに 1 つだけなので、既に動いているタイマーは置き換える。
pub(super) async fn start_after_set_completed(
    training_id: String,
    training_exercise_id: String,
    set_id: String,
    state: RestTimerState,
) {
    let Ok(storage) = storage::current().await else {
        return;
    };
    let default_seconds = storage
        .rest_default_seconds()
        .await
        .unwrap_or(DEFAULT_SECONDS);
    let exercise_seconds = storage
        .rest_exercise_seconds(&training_exercise_id)
        .await
        .ok()
        .flatten();
    let set_seconds = storage.rest_set_seconds(&set_id).await.ok().flatten();
    let seconds = core_timer::resolve_seconds(default_seconds, exercise_seconds, set_seconds)
        .unwrap_or(DEFAULT_SECONDS);
    let Some(timer) = RestTimer::start(now_ms(), seconds) else {
        return;
    };
    state.start(&training_id, timer);
    let _ = storage.save_rest_timer(&training_id, timer.end_ms()).await;
}
