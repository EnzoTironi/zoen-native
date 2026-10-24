//! Bounded same-process ABI. The JS controller checks byte limits before the
//! wasm-bindgen copy; the queue never accepts document URLs or network authority.
use crate::layout::Layout;
use js_sys::{Function, Promise};
use photocraft_ui_egui::PhotocraftApp;
use photocraft_zoen_host::managed::{Accept, Command, Export, Mutation, Open, Proposal};
use serde::Deserialize;
use serde_json::json;
use std::{cell::RefCell, collections::VecDeque};
use wasm_bindgen::prelude::*;

const MAX_DOCUMENT: usize = photocraft_zoen_host::MAX_DOCUMENT_BYTES;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Freeze {
    frozen: bool,
}
#[derive(Deserialize)]
#[serde(
    tag = "op",
    content = "input",
    rename_all = "camelCase",
    deny_unknown_fields
)]
enum Request {
    Open(Open),
    Command(Command),
    Snapshot(Mutation),
    Accept(Accept),
    Export(Export),
    Freeze(Freeze),
    Layout(Layout),
    Status,
}
struct Pending {
    request: Request,
    bytes: Vec<u8>,
    resolve: Function,
    reject: Function,
}
#[derive(Default)]
struct Queue {
    started: bool,
    retired: bool,
    ctx: Option<egui::Context>,
    pending: VecDeque<Pending>,
    bytes: usize,
    on_change: Option<Function>,
    on_export: Option<Function>,
    layout: Option<Layout>,
    reported: Option<String>,
}
thread_local! { static QUEUE:RefCell<Queue> = RefCell::new(Queue::default()); }
fn error(code: &str) -> JsValue {
    JsValue::from_str(&json!({"code":code}).to_string())
}

#[wasm_bindgen(js_name=zoenHostStart)]
pub fn start() -> Promise {
    let admitted = QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        if q.started || q.retired {
            false
        } else {
            q.started = true;
            true
        }
    });
    if !admitted {
        return Promise::reject(&error("host_started_or_retired"));
    }
    wasm_bindgen_futures::future_to_promise(async {
        match crate::shell::start().await {
            Ok(()) => Ok(JsValue::UNDEFINED),
            Err(e) => {
                retire();
                Err(e)
            }
        }
    })
}
pub(crate) fn install(ctx: egui::Context) {
    QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        if q.started && !q.retired {
            q.ctx = Some(ctx);
        }
    });
}
pub(crate) fn retired() -> bool {
    QUEUE.with(|q| q.borrow().retired)
}
pub(crate) fn layout() -> Option<Layout> {
    QUEUE.with(|q| q.borrow().layout)
}

#[wasm_bindgen(js_name=zoenHostRequest)]
pub fn request(envelope: &str, bytes: Vec<u8>) -> Promise {
    if envelope.len() > 65_536 || bytes.len() > MAX_DOCUMENT {
        return Promise::reject(&error("request_too_large"));
    }
    let Ok(request) = serde_json::from_str::<Request>(envelope) else {
        return Promise::reject(&error("invalid_request"));
    };
    if !matches!(request, Request::Open(_)) && !bytes.is_empty() {
        return Promise::reject(&error("unexpected_binary_input"));
    }
    let mut input = Some((request, bytes));
    Promise::new(&mut |resolve, reject| {
        let Some((request, bytes)) = input.take() else {
            return;
        };
        let refused = QUEUE.with(|q| {
            let mut q = q.borrow_mut();
            if !q.started
                || q.retired
                || q.ctx.is_none()
                || q.pending.len() >= 8
                || q.bytes.saturating_add(bytes.len()) > MAX_DOCUMENT * 2
            {
                return true;
            }
            q.bytes += bytes.len();
            q.pending.push_back(Pending {
                request,
                bytes,
                resolve,
                reject: reject.clone(),
            });
            if let Some(ctx) = &q.ctx {
                ctx.request_repaint();
            }
            false
        });
        if refused {
            let _ = reject.call1(&JsValue::NULL, &error("host_unavailable_or_busy"));
        }
    })
}
#[wasm_bindgen(js_name=zoenHostOnChange)]
/// # Errors
/// Refuses callback replacement before startup or after owner retirement.
pub fn on_change(callback: Option<Function>) -> Result<(), JsValue> {
    QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        if !q.started || q.retired {
            return Err(error("host_unavailable_or_retired"));
        }
        q.on_change = callback;
        Ok(())
    })
}
#[wasm_bindgen(js_name=zoenHostOnExport)]
/// # Errors
/// Refuses callback replacement before startup or after owner retirement.
pub fn on_export(callback: Option<Function>) -> Result<(), JsValue> {
    QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        if !q.started || q.retired {
            return Err(error("host_unavailable_or_retired"));
        }
        q.on_export = callback;
        Ok(())
    })
}
#[wasm_bindgen(js_name=zoenHostRetire)]
pub fn retire() {
    let pending = QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        q.retired = true;
        q.bytes = 0;
        q.on_change = None;
        q.on_export = None;
        q.reported = None;
        if let Some(ctx) = q.ctx.take() {
            ctx.request_repaint();
        }
        std::mem::take(&mut q.pending)
    });
    for mut p in pending {
        p.bytes.fill(0);
        let _ = p.reject.call1(&JsValue::NULL, &error("host_retired"));
    }
}
pub(crate) fn changed(bridge: &photocraft_zoen_host::browser::BrowserBridge, app: &PhotocraftApp) {
    let Ok(status) = bridge.status(&app.session) else {
        return;
    };
    let status = status.to_string();
    let callback = QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        if q.retired || q.reported.as_ref() == Some(&status) {
            return None;
        }
        q.reported = Some(status.clone());
        q.on_change.clone()
    });
    if let Some(f) = callback {
        let _ = f.call1(&JsValue::NULL, &JsValue::from_str(&status));
    }
}
pub(crate) fn drain(
    bridge: &mut photocraft_zoen_host::browser::BrowserBridge,
    app: &mut PhotocraftApp,
) {
    let work = QUEUE.with(|q| {
        let mut q = q.borrow_mut();
        let p = q.pending.pop_front();
        if let Some(p) = &p {
            q.bytes = q.bytes.saturating_sub(p.bytes.len());
        }
        p
    });
    let Some(mut work) = work else {
        return;
    };
    let interaction = app.host_interaction()
        || QUEUE.with(|q| {
            q.borrow().ctx.as_ref().is_some_and(|ctx| {
                ctx.input(|input| input.pointer.any_down() || input.multi_touch().is_some())
            })
        });
    let empty = |metadata| Proposal {
        bytes: vec![],
        metadata,
    };
    let result = match work.request {
        Request::Open(_) | Request::Command(_) | Request::Snapshot(_) if interaction => {
            Err("human_interaction".into())
        }
        Request::Open(r) => bridge.open(&mut app.session, r, &work.bytes).map(empty),
        Request::Command(r) => bridge.execute(&mut app.session, r),
        Request::Snapshot(r) => bridge.snapshot(&mut app.session, r),
        Request::Accept(r) => bridge.accept(&mut app.session, &r).map(empty),
        Request::Export(r) => bridge.export(&app.session, &r),
        Request::Status => bridge.status(&app.session).map(empty),
        Request::Freeze(r) => {
            if r.frozen && interaction {
                Err("human_interaction".into())
            } else {
                bridge.freeze(&mut app.session, r.frozen).map(empty)
            }
        }
        Request::Layout(r) => {
            if r.valid() {
                QUEUE.with(|q| q.borrow_mut().layout = Some(r));
                Ok(empty(json!({"layoutAccepted":true})))
            } else {
                Err("invalid_host_layout".into())
            }
        }
    };
    work.bytes.fill(0);
    match result {
        Ok(p) => {
            let o = js_sys::Object::new();
            let wrapped = js_sys::Reflect::set(
                &o,
                &"metadata".into(),
                &JsValue::from_str(&p.metadata.to_string()),
            )
            .and_then(|_| {
                js_sys::Reflect::set(
                    &o,
                    &"bytes".into(),
                    &js_sys::Uint8Array::from(p.bytes.as_slice()),
                )
            });
            match wrapped {
                Ok(_) => {
                    let _ = work.resolve.call1(&JsValue::NULL, &o);
                }
                Err(e) => {
                    let _ = work.reject.call1(&JsValue::NULL, &e);
                }
            }
        }
        Err(e) => {
            let _ = work.reject.call1(&JsValue::NULL, &error(&e));
        }
    }
    QUEUE.with(|q| {
        let q = q.borrow();
        if !q.pending.is_empty()
            && let Some(ctx) = &q.ctx
        {
            ctx.request_repaint();
        }
    });
}
