//! The browser inside a browser microVM (ADR 0028 §7).
//!
//! - **CDP stays inside.** Chromium runs with `--remote-debugging-pipe`: the DevTools protocol
//!   is a pair of pipes between Chromium and this process, with no port anyone could reach.
//!   Only the operations in [`BrowserOp`] cross vsock.
//! - **Network.** Chromium's only proxy is the in-VM proxy address, which leads to the host's
//!   egress proxy; it trusts the node's sandbox root (added to its NSS store before launch),
//!   under which each browser lease's name-constrained CA is issued.
//! - **Live view.** Screencast frames are sealed here to the owner's device key
//!   ([`zoen_liveview`]) and streamed to the host on [`LIVE_PORT`]; the host and the model see
//!   ciphertext only. The model never gets frames.
//! - **Takeover.** While the owner has the browser, every agent operation is refused (and so
//!   are exec and file access). Input arrives sealed by the device; replays and anything the
//!   host makes up are refused; only the device's sealed `Done` ends it. Text the owner typed
//!   is kept in memory only to scrub it from anything the agent later reads; it is never
//!   written anywhere.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use zoen_guestd::*;
use zoen_liveview::{Direction, InputEvent, Opener, Sealer};

mod bootstrap;
use bootstrap::{launch, LOCATE_JS, READ_JS};

const VIEW_W: u32 = 1280;
const VIEW_H: u32 = 800;
const NAV_GRACE: Duration = Duration::from_millis(150);

/// The DevTools connection over the pipe.
struct Cdp {
    to_chrome: Mutex<File>,
    next: AtomicU64,
    pending: Mutex<HashMap<u64, mpsc::Sender<Value>>>,
    /// Where screencast frames go while a live view runs.
    frames: Mutex<Option<mpsc::SyncSender<Vec<u8>>>>,
}

impl Cdp {
    fn send(&self, method: &str, params: Value, session: Option<&str>) -> Result<u64, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        let mut bytes = serde_json::to_vec(&msg).map_err(|e| e.to_string())?;
        bytes.push(0);
        self.to_chrome
            .lock()
            .unwrap()
            .write_all(&bytes)
            .map_err(|e| format!("browser pipe: {e}"))?;
        Ok(id)
    }

    fn call(
        &self,
        method: &str,
        params: Value,
        session: Option<&str>,
        limit: Duration,
    ) -> Result<Value, String> {
        let (tx, rx) = mpsc::channel();
        // Register before sending, so a fast answer isn't lost.
        let id = {
            let mut p = self.pending.lock().unwrap();
            let id = self.next.fetch_add(1, Ordering::Relaxed);
            p.insert(id, tx);
            id
        };
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        let mut bytes = serde_json::to_vec(&msg).map_err(|e| e.to_string())?;
        bytes.push(0);
        if let Err(e) = self.to_chrome.lock().unwrap().write_all(&bytes) {
            self.pending.lock().unwrap().remove(&id);
            return Err(format!("browser pipe: {e}"));
        }
        let r = rx.recv_timeout(limit);
        self.pending.lock().unwrap().remove(&id);
        let r = r.map_err(|_| format!("browser did not answer {method}"))?;
        if let Some(e) = r.get("error") {
            return Err(e["message"].as_str().unwrap_or("browser error").to_string());
        }
        Ok(r.get("result").cloned().unwrap_or(Value::Null))
    }

    fn read_loop(self: Arc<Self>, mut from_chrome: File) {
        let mut buf = Vec::with_capacity(1 << 20);
        let mut chunk = vec![0u8; 256 * 1024];
        loop {
            let n = match from_chrome.read(&mut chunk) {
                Ok(0) | Err(_) => return,
                Ok(n) => n,
            };
            buf.extend_from_slice(&chunk[..n]);
            while let Some(i) = buf.iter().position(|b| *b == 0) {
                let msg: Vec<u8> = buf.drain(..=i).collect();
                let Ok(v) = serde_json::from_slice::<Value>(&msg[..msg.len() - 1]) else {
                    continue;
                };
                if let Some(id) = v.get("id").and_then(Value::as_u64) {
                    if let Some(tx) = self.pending.lock().unwrap().remove(&id) {
                        let _ = tx.send(v);
                    }
                } else if v["method"] == "Page.screencastFrame" {
                    self.on_frame(&v);
                }
            }
        }
    }

    fn on_frame(&self, v: &Value) {
        let p = &v["params"];
        let session = v["sessionId"].as_str();
        // Ack first so Chromium keeps sending.
        let _ = self.send(
            "Page.screencastFrameAck",
            json!({"sessionId": p["sessionId"]}),
            session,
        );
        let Some(data) = p["data"].as_str().and_then(|d| B64.decode(d).ok()) else {
            return;
        };
        if let Some(tx) = self.frames.lock().unwrap().as_ref() {
            // A slow viewer drops frames rather than holding the browser up.
            let _ = tx.try_send(data);
        }
    }
}

struct Live {
    input: Opener,
    stop: Arc<AtomicBool>,
}

pub struct Browser {
    cdp: Arc<Cdp>,
    session: String,
    takeover: AtomicBool,
    live: Mutex<Option<Live>>,
    /// What the owner typed during takeovers: scrubbed from everything the agent reads.
    owner_typed: Mutex<Vec<String>>,
    /// One agent operation at a time.
    op: Mutex<()>,
}

static BROWSER: OnceLock<Arc<Browser>> = OnceLock::new();

/// Whether the owner has the browser right now (exec and file access are refused then too).
pub fn takeover_active() -> bool {
    BROWSER
        .get()
        .is_some_and(|b| b.takeover.load(Ordering::SeqCst))
}

pub fn dispatch(op: BrowserOp) -> Response {
    let r = match op {
        BrowserOp::Launch { root_ca_pem } => launch(root_ca_pem),
        other => match BROWSER.get() {
            None => Err("no browser in this VM".to_string()),
            Some(b) => b.clone().handle(other),
        },
    };
    match r {
        Ok(v) => Response::value(v),
        Err(e) => Response::err(e),
    }
}

impl Browser {
    fn t(&self, ms: u64) -> Duration {
        Duration::from_millis(ms.clamp(1000, 120_000))
    }

    fn page(&self, method: &str, params: Value, limit: Duration) -> Result<Value, String> {
        self.cdp.call(method, params, Some(&self.session), limit)
    }

    fn eval(&self, expr: &str, limit: Duration) -> Result<Value, String> {
        let r = self.page(
            "Runtime.evaluate",
            json!({"expression": expr, "returnByValue": true, "awaitPromise": true}),
            limit,
        )?;
        if let Some(e) = r.get("exceptionDetails") {
            return Err(format!(
                "page script failed: {}",
                e["exception"]["description"].as_str().unwrap_or("error")
            ));
        }
        Ok(r["result"]["value"].clone())
    }

    /// Waits for the document to finish loading. `grace`: how long a navigation a click or a
    /// key started may take to begin (`Page.navigate` returns after it has).
    fn settle(&self, grace: Duration, limit: Duration) {
        let deadline = Instant::now() + limit;
        std::thread::sleep(grace);
        while Instant::now() < deadline {
            match self.eval("document.readyState", Duration::from_secs(5)) {
                Ok(v) if v == "complete" => return,
                _ => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }

    /// Replaces what the owner typed, wherever it shows up in a result for the agent.
    fn scrub(&self, v: Value) -> Value {
        let typed = self.owner_typed.lock().unwrap().clone();
        fn walk(v: Value, typed: &[String]) -> Value {
            match v {
                Value::String(mut s) => {
                    for t in typed {
                        s = s.replace(t.as_str(), "[hidden]");
                    }
                    Value::String(s)
                }
                Value::Array(a) => Value::Array(a.into_iter().map(|x| walk(x, typed)).collect()),
                Value::Object(o) => {
                    Value::Object(o.into_iter().map(|(k, x)| (k, walk(x, typed))).collect())
                }
                other => other,
            }
        }
        walk(v, &typed)
    }

    fn click_at(&self, x: f64, y: f64) -> Result<(), String> {
        let t = Duration::from_secs(5);
        for (kind, buttons) in [("mouseMoved", 0), ("mousePressed", 1), ("mouseReleased", 0)] {
            let mut p = json!({"type": kind, "x": x, "y": y, "buttons": buttons});
            if kind != "mouseMoved" {
                p["button"] = json!("left");
                p["clickCount"] = json!(1);
            }
            self.page("Input.dispatchMouseEvent", p, t)?;
        }
        Ok(())
    }

    fn key(&self, key: &str) -> Result<(), String> {
        let (code, vk, text) = match key {
            "Enter" => ("Enter", 13, "\r"),
            "Tab" => ("Tab", 9, ""),
            "Backspace" => ("Backspace", 8, ""),
            "Escape" => ("Escape", 27, ""),
            "ArrowDown" => ("ArrowDown", 40, ""),
            "ArrowUp" => ("ArrowUp", 38, ""),
            "ArrowLeft" => ("ArrowLeft", 37, ""),
            "ArrowRight" => ("ArrowRight", 39, ""),
            _ => return Err(format!("unknown key {key}")),
        };
        let t = Duration::from_secs(5);
        let mut down = json!({"type": "keyDown", "key": key, "code": code,
                              "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk});
        if !text.is_empty() {
            down["text"] = json!(text);
        }
        self.page("Input.dispatchKeyEvent", down, t)?;
        self.page(
            "Input.dispatchKeyEvent",
            json!({"type": "keyUp", "key": key, "code": code,
                   "windowsVirtualKeyCode": vk, "nativeVirtualKeyCode": vk}),
            t,
        )?;
        Ok(())
    }

    fn locate(&self, target: &str) -> Result<(f64, f64, bool), String> {
        let expr = format!("{LOCATE_JS}({})", json!(target));
        let v = self.eval(&expr, Duration::from_secs(10))?;
        if v.is_null() {
            return Err(format!(
                "no element `{target}` on the page (read it again?)"
            ));
        }
        Ok((
            v["x"].as_f64().unwrap_or(0.0),
            v["y"].as_f64().unwrap_or(0.0),
            v["secret"].as_bool().unwrap_or(false),
        ))
    }

    fn agent_op_allowed(&self) -> Result<(), String> {
        if self.takeover.load(Ordering::SeqCst) {
            return Err(format!(
                "{TAKEOVER_IN_PROGRESS}: the owner is using the browser; wait until they finish"
            ));
        }
        Ok(())
    }

    fn handle(self: Arc<Self>, op: BrowserOp) -> Result<Value, String> {
        match op {
            BrowserOp::Launch { .. } => unreachable!(),
            BrowserOp::Status => Ok(json!({
                "takeover": self.takeover.load(Ordering::SeqCst),
                "live": self.live.lock().unwrap().is_some(),
            })),
            BrowserOp::LiveStart { device_pub_b64 } => self.live_start(&device_pub_b64),
            BrowserOp::LiveStop => {
                self.live_stop();
                Ok(json!({}))
            }
            BrowserOp::TakeoverBegin => {
                if self.live.lock().unwrap().is_none() {
                    return Err("start the live view first: the owner must see the page".into());
                }
                self.takeover.store(true, Ordering::SeqCst);
                Ok(json!({"takeover": true}))
            }
            BrowserOp::TakeoverInput { sealed_b64 } => self.takeover_input(&sealed_b64),
            agent => {
                let _one = self.op.lock().unwrap();
                self.agent_op_allowed()?;
                let v = self.agent(agent)?;
                // A takeover may have started while this ran: then the agent gets nothing.
                self.agent_op_allowed()?;
                Ok(self.scrub(v))
            }
        }
    }

    fn agent(&self, op: BrowserOp) -> Result<Value, String> {
        match op {
            BrowserOp::Open { url, timeout_ms } => {
                if !(url.starts_with("https://") || url.starts_with("http://")) {
                    return Err("only http and https URLs".into());
                }
                let t = self.t(timeout_ms);
                let r = self.page("Page.navigate", json!({"url": url}), t)?;
                if let Some(e) = r["errorText"].as_str() {
                    return Err(format!("could not open {url}: {e}"));
                }
                self.settle(Duration::ZERO, t);
                let v = self.eval("({url: location.href, title: document.title})", t)?;
                Ok(v)
            }
            BrowserOp::Read { max_chars } => {
                let v = self.eval(READ_JS, Duration::from_secs(20))?;
                let mut text = format!(
                    "{}\n{}\n\n{}\n\nElements:\n{}",
                    v["title"].as_str().unwrap_or(""),
                    v["url"].as_str().unwrap_or(""),
                    v["text"].as_str().unwrap_or("").trim(),
                    v["elements"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                let max = max_chars.clamp(1000, 200_000);
                if text.chars().count() > max {
                    text = text.chars().take(max).collect::<String>() + "\n[cut]";
                }
                Ok(json!({"url": v["url"], "title": v["title"], "text": text}))
            }
            BrowserOp::Click { target, timeout_ms } => {
                let (x, y, _) = self.locate(&target)?;
                self.click_at(x, y)?;
                self.settle(NAV_GRACE, self.t(timeout_ms));
                self.eval(
                    "({url: location.href, title: document.title})",
                    Duration::from_secs(10),
                )
            }
            BrowserOp::Type {
                target,
                text,
                submit,
                timeout_ms,
            } => {
                let (x, y, secret) = self.locate(&target)?;
                if secret {
                    return Err(format!(
                        "{FIELD_NEEDS_OWNER}: passwords and codes are typed by the owner; ask for a takeover"
                    ));
                }
                self.click_at(x, y)?;
                // Replace what's there.
                self.eval(
                    "(() => { const e = document.activeElement; if (e && 'value' in e) { e.value = ''; } })()",
                    Duration::from_secs(5),
                )?;
                self.page(
                    "Input.insertText",
                    json!({"text": text}),
                    Duration::from_secs(10),
                )?;
                if submit {
                    self.key("Enter")?;
                }
                self.settle(NAV_GRACE, self.t(timeout_ms));
                self.eval(
                    "({url: location.href, title: document.title})",
                    Duration::from_secs(10),
                )
            }
            _ => Err("not an agent operation".into()),
        }
    }

    fn live_start(&self, device_pub_b64: &str) -> Result<Value, String> {
        let device: [u8; 32] = B64
            .decode(device_pub_b64)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or("bad device key")?;
        self.live_stop();
        let (offer, keys) = zoen_liveview::vm_start(&device).map_err(|error| error.to_string())?;
        let mut conn = super::linux::vsock_connect_host(LIVE_PORT)
            .map_err(|e| format!("live view channel: {e}"))?;
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(8);
        let stop = Arc::new(AtomicBool::new(false));
        let mut sealer: Sealer = keys.sealer(Direction::Frames);
        let stop2 = stop.clone();
        std::thread::spawn(move || {
            while let Ok(frame) = rx.recv() {
                if stop2.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(sealed) = sealer.seal(&frame) else {
                    break;
                };
                let len = (sealed.len() as u32).to_be_bytes();
                if conn
                    .write_all(&len)
                    .and_then(|_| conn.write_all(&sealed))
                    .is_err()
                {
                    break;
                }
            }
            // SAFETY: shutting down our own socket.
            unsafe { libc::shutdown(std::os::fd::AsRawFd::as_raw_fd(&conn), libc::SHUT_RDWR) };
        });
        *self.cdp.frames.lock().unwrap() = Some(tx);
        *self.live.lock().unwrap() = Some(Live {
            input: keys.opener(Direction::Input),
            stop,
        });
        self.page(
            "Page.startScreencast",
            json!({"format": "jpeg", "quality": 60, "maxWidth": VIEW_W, "maxHeight": VIEW_H,
                   "everyNthFrame": 1}),
            Duration::from_secs(10),
        )?;
        Ok(json!({
            "vm_pub_b64": B64.encode(offer.vm_pub),
            "session_b64": B64.encode(offer.session),
        }))
    }

    fn live_stop(&self) {
        let _ = self.page("Page.stopScreencast", json!({}), Duration::from_secs(5));
        *self.cdp.frames.lock().unwrap() = None;
        if let Some(l) = self.live.lock().unwrap().take() {
            l.stop.store(true, Ordering::SeqCst);
        }
    }

    fn takeover_input(&self, sealed_b64: &str) -> Result<Value, String> {
        if !self.takeover.load(Ordering::SeqCst) {
            return Err("no takeover in progress".into());
        }
        let sealed = B64.decode(sealed_b64).map_err(|_| "bad input")?;
        let event = {
            let mut live = self.live.lock().unwrap();
            let live = live.as_mut().ok_or("no live session")?;
            let plain = live.input.open(&sealed).map_err(|e| e.to_string())?;
            serde_json::from_slice::<InputEvent>(&plain).map_err(|_| "bad input event")?
        };
        match event {
            InputEvent::Text { text } => {
                self.page(
                    "Input.insertText",
                    json!({"text": text}),
                    Duration::from_secs(10),
                )?;
                if text.chars().count() >= 3 {
                    self.owner_typed.lock().unwrap().push(text);
                }
            }
            InputEvent::Click { x, y } => self.click_at(x, y)?,
            InputEvent::ClickSelector { selector } => {
                let (x, y, _) = self.locate(&selector)?;
                self.click_at(x, y)?;
            }
            InputEvent::Key { key } => {
                self.key(&key)?;
                if key == "Enter" {
                    self.settle(NAV_GRACE, Duration::from_secs(10));
                }
            }
            InputEvent::Done => {
                self.takeover.store(false, Ordering::SeqCst);
                return Ok(json!({"done": true}));
            }
        }
        Ok(json!({"done": false}))
    }
}
