//! 記録中の画面の自動消灯を防ぐ Wake Lock の要求と取り直し (REQ-TMR-005)。
//!
//! ブラウザの Screen Wake Lock API を `navigator.wakeLock.request("screen")` で要求する。
//! 画面が隠れるとブラウザはロックを解放するため、表示に戻ったときに取り直す。
//! API が無いブラウザでは何もしない。設定で無効にしたときはロックを解放し、
//! 端末の通常の消灯に戻す。

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use js_sys::{Function, Promise, Reflect};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{Document, Event, VisibilityState};

/// 画面の自動消灯を防ぐロック。
#[derive(Clone, Default)]
pub struct ScreenWakeLock {
    inner: Rc<RefCell<Inner>>,
}

#[derive(Default)]
struct Inner {
    /// ブラウザが返したロック。解放済みなら None。
    sentinel: Option<JsValue>,
    /// 要求の世代。停止した後に届いた要求の結果を捨てる。
    generation: u64,
    /// `visibilitychange` の listener。
    listener: Option<Closure<dyn FnMut(Event)>>,
}

impl ScreenWakeLock {
    /// ロックを要求する。既にロックを持っているときは何もしない。
    pub fn start(&self) {
        install_listener(&self.inner);
        let generation = {
            let mut inner = self.inner.borrow_mut();
            inner.generation = inner.generation.wrapping_add(1);
            inner.generation
        };
        drop_released_lock(&self.inner);
        if self.inner.borrow().sentinel.is_some() {
            return;
        }
        request_lock(self.inner.clone(), generation);
    }

    /// ロックを解放し、端末の通常の消灯に戻す。
    pub fn stop(&self) {
        remove_listener(&self.inner);
        let sentinel = {
            let mut inner = self.inner.borrow_mut();
            inner.generation = inner.generation.wrapping_add(1);
            inner.sentinel.take()
        };
        if let Some(sentinel) = sentinel {
            release_lock(&sentinel);
        }
    }

    fn reacquire(&self) {
        drop_released_lock(&self.inner);
        if self.inner.borrow().sentinel.is_some() {
            return;
        }
        let generation = self.inner.borrow().generation;
        request_lock(self.inner.clone(), generation);
    }
}

/// ブラウザが解放したロックを状態から消す。
fn drop_released_lock(inner: &Rc<RefCell<Inner>>) {
    let released = inner
        .borrow()
        .sentinel
        .as_ref()
        .is_some_and(sentinel_released);
    if released {
        inner.borrow_mut().sentinel = None;
    }
}

fn install_listener(inner: &Rc<RefCell<Inner>>) {
    if inner.borrow().listener.is_some() {
        return;
    }
    let Some(document) = document() else {
        return;
    };
    let weak: Weak<RefCell<Inner>> = Rc::downgrade(inner);
    let listener = Closure::<dyn FnMut(Event)>::new(move |_event: Event| {
        if !document_is_visible() {
            return;
        }
        if let Some(inner) = weak.upgrade() {
            ScreenWakeLock { inner }.reacquire();
        }
    });
    if document
        .add_event_listener_with_callback("visibilitychange", listener.as_ref().unchecked_ref())
        .is_ok()
    {
        inner.borrow_mut().listener = Some(listener);
    }
}

fn remove_listener(inner: &Rc<RefCell<Inner>>) {
    let listener = inner.borrow_mut().listener.take();
    let (Some(listener), Some(document)) = (listener, document()) else {
        return;
    };
    // 画面を離れるときの後始末であり、削除の失敗で記録を止めない。
    let _ = document
        .remove_event_listener_with_callback("visibilitychange", listener.as_ref().unchecked_ref());
}

fn request_lock(inner: Rc<RefCell<Inner>>, generation: u64) {
    spawn_local(async move {
        let Some(sentinel) = request_sentinel().await else {
            return;
        };
        let accepted = {
            let mut inner = inner.borrow_mut();
            if inner.generation != generation || inner.sentinel.is_some() {
                false
            } else {
                inner.sentinel = Some(sentinel.clone());
                true
            }
        };
        if !accepted {
            release_lock(&sentinel);
        }
    });
}

async fn request_sentinel() -> Option<JsValue> {
    let window = web_sys::window()?;
    let wake_lock = Reflect::get(&window.navigator(), &JsValue::from_str("wakeLock")).ok()?;
    if wake_lock.is_undefined() || wake_lock.is_null() {
        return None;
    }
    let request = Reflect::get(&wake_lock, &JsValue::from_str("request")).ok()?;
    let request: &Function = request.dyn_ref()?;
    let promise = request
        .call1(&wake_lock, &JsValue::from_str("screen"))
        .ok()?;
    let promise: Promise = promise.dyn_into().ok()?;
    JsFuture::from(promise).await.ok()
}

fn release_lock(sentinel: &JsValue) {
    let Ok(release) = Reflect::get(sentinel, &JsValue::from_str("release")) else {
        return;
    };
    let Some(release) = release.dyn_ref::<Function>() else {
        return;
    };
    // 解放の失敗で記録を止めない。画面を離れるときにブラウザが解放する。
    let _ = release.call0(sentinel);
}

fn sentinel_released(sentinel: &JsValue) -> bool {
    Reflect::get(sentinel, &JsValue::from_str("released"))
        .ok()
        .and_then(|released| released.as_bool())
        .unwrap_or(false)
}

fn document() -> Option<Document> {
    web_sys::window()?.document()
}

fn document_is_visible() -> bool {
    document().is_some_and(|document| document.visibility_state() == VisibilityState::Visible)
}
