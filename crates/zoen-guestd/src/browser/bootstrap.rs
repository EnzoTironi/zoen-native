const CHROME: &str = "/usr/lib/chromium/chrome";
const NSSDB: &str = "sql:/root/.pki/nssdb";
const PROFILE: &str = "/work/.chromium";

use super::{Browser, Cdp, BROWSER, VIEW_H, VIEW_W};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::File;
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zoen_guestd::GUEST_PROXY;

fn run(argv: &[&str]) -> Result<(), String> {
    let st = Command::new(argv[0])
        .args(&argv[1..])
        .env("HOME", "/root")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("{}: {e}", argv[0]))?;
    st.success()
        .then_some(())
        .ok_or_else(|| format!("{} failed", argv[0]))
}

fn pipe() -> Result<(RawFd, RawFd), String> {
    let mut fds = [0; 2];
    // SAFETY: valid two-element array.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    // Move both ends above 10, so the child's dup2 onto 3 and 4 can't clobber them.
    let hi = |fd: RawFd| {
        // SAFETY: fcntl on an fd we own; then close the original.
        let n = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 10) };
        unsafe { libc::close(fd) };
        n
    };
    Ok((hi(fds[0]), hi(fds[1])))
}

pub(super) fn launch(root_ca_pem: Option<String>) -> Result<Value, String> {
    if BROWSER.get().is_some() {
        return Err("the browser is already running".into());
    }
    super::super::linux::mount_tmpfs("/dev/shm", "mode=1777,size=256m");
    std::fs::create_dir_all("/root/.pki/nssdb").map_err(|e| e.to_string())?;
    if !std::path::Path::new("/root/.pki/nssdb/cert9.db").exists() {
        run(&["/usr/bin/certutil", "-N", "-d", NSSDB, "--empty-password"])?;
    }
    if let Some(pem) = root_ca_pem {
        std::fs::write("/run/zoen/sandbox-root.crt", pem).map_err(|e| e.to_string())?;
        run(&[
            "/usr/bin/certutil",
            "-A",
            "-d",
            NSSDB,
            "-n",
            "zoen-sandbox-root",
            "-t",
            "C,,",
            "-i",
            "/run/zoen/sandbox-root.crt",
        ])?;
    }
    let (in_r, in_w) = pipe()?; // we write, Chromium reads fd 3
    let (out_r, out_w) = pipe()?; // Chromium writes fd 4, we read
    let mut cmd = Command::new(CHROME);
    cmd.args([
        "--headless=new",
        // The microVM is the sandbox: Chromium's own needs user namespaces it can't have as
        // PID-1's child here.
        "--no-sandbox",
        "--remote-debugging-pipe",
        &format!("--user-data-dir={PROFILE}"),
        &format!("--proxy-server=http://{GUEST_PROXY}"),
        "--proxy-bypass-list=<-loopback>",
        &format!("--window-size={VIEW_W},{VIEW_H}"),
        "--disable-gpu",
        "--disable-dev-shm-usage",
        "--no-first-run",
        "--no-default-browser-check",
        // Nothing of its own on the network: every request is the agent's or the owner's.
        "--disable-background-networking",
        "--disable-component-update",
        "--disable-domain-reliability",
        "--disable-client-side-phishing-detection",
        "--disable-sync",
        "--no-pings",
        "--metrics-recording-only",
        "--disable-features=Translate,OptimizationHints,MediaRouter,AutofillServerCommunication",
        "--password-store=basic",
        "--mute-audio",
        "about:blank",
    ])
    .env_clear()
    .env("HOME", "/root")
    .env("PATH", "/usr/bin:/bin")
    .env("TMPDIR", "/tmp")
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    // SAFETY: only async-signal-safe calls between fork and exec.
    unsafe {
        cmd.pre_exec(move || {
            if libc::dup2(in_r, 3) < 0 || libc::dup2(out_w, 4) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = cmd.spawn().map_err(|e| format!("start chromium: {e}"));
    // SAFETY: the child has its copies; close ours.
    unsafe {
        libc::close(in_r);
        libc::close(out_w);
    }
    let _child = child?;
    // SAFETY: we own these fds from here on.
    let (to, from) = unsafe { (File::from_raw_fd(in_w), File::from_raw_fd(out_r)) };
    let cdp = Arc::new(Cdp {
        to_chrome: Mutex::new(to),
        next: AtomicU64::new(1),
        pending: Mutex::new(HashMap::new()),
        frames: Mutex::new(None),
    });
    let reader = cdp.clone();
    std::thread::spawn(move || reader.read_loop(from));

    let t = Duration::from_secs(10);
    let deadline = Instant::now() + Duration::from_secs(60);
    let version = loop {
        match cdp.call("Browser.getVersion", json!({}), None, t) {
            Ok(v) => break v,
            Err(e) if Instant::now() > deadline => return Err(format!("chromium: {e}")),
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    };
    let targets = cdp.call("Target.getTargets", json!({}), None, t)?;
    let page = targets["targetInfos"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["type"] == "page")
        .and_then(|t| t["targetId"].as_str().map(str::to_string));
    let page = match page {
        Some(p) => p,
        None => cdp.call(
            "Target.createTarget",
            json!({"url": "about:blank"}),
            None,
            t,
        )?["targetId"]
            .as_str()
            .ok_or("no page target")?
            .to_string(),
    };
    let session = cdp.call(
        "Target.attachToTarget",
        json!({"targetId": page, "flatten": true}),
        None,
        t,
    )?["sessionId"]
        .as_str()
        .ok_or("no session")?
        .to_string();
    cdp.call("Page.enable", json!({}), Some(&session), t)?;
    let b = Arc::new(Browser {
        cdp,
        session,
        takeover: AtomicBool::new(false),
        live: Mutex::new(None),
        owner_typed: Mutex::new(vec![]),
        op: Mutex::new(()),
    });
    let _ = BROWSER.set(b);
    Ok(json!({"product": version["product"]}))
}

/// The page snapshot the agent reads. Field values of password and one-time-code inputs are
/// never read out; elements get numbers the agent can click and type into.
pub(super) const READ_JS: &str = r#"(() => {
  const secret = el => {
    const t = (el.type || '').toLowerCase();
    const ac = (el.autocomplete || '').toLowerCase();
    const n = ((el.name || '') + ' ' + (el.id || '')).toLowerCase();
    return t === 'password' || ac.includes('one-time-code') || ac.includes('password') ||
      ac.startsWith('cc-') || /otp|totp|2fa|mfa|one.?time|passcode|\bpin\b|cvc|cvv/.test(n);
  };
  const out = [];
  let i = 0;
  const sel = 'a[href],button,input,textarea,select,[role=button],[contenteditable=true]';
  for (const el of document.querySelectorAll(sel)) {
    if ((el.type || '') === 'hidden') continue;
    const r = el.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) continue;
    i += 1;
    el.setAttribute('data-zoen-ref', String(i));
    const tag = el.tagName.toLowerCase();
    let d;
    if (tag === 'input' || tag === 'textarea' || tag === 'select') {
      const kind = tag === 'input' ? (el.type || 'text') : tag;
      const v = secret(el) ? (el.value ? '[hidden]' : '') : (el.value || '');
      const label = (el.labels && el.labels[0] ? el.labels[0].innerText : '') ||
        el.placeholder || el.getAttribute('aria-label') || el.name || el.id || '';
      d = `${kind} "${label.trim().slice(0, 60)}"` + (v ? ` = "${v.slice(0, 200)}"` : '') +
        (secret(el) ? ' (owner only)' : '');
    } else {
      const kind = tag === 'a' ? 'link' : 'button';
      const text = (el.innerText || el.value || el.getAttribute('aria-label') || '').trim();
      d = `${kind} "${text.replace(/\s+/g, ' ').slice(0, 80)}"`;
    }
    out.push(`[${i}] ${d}`);
  }
  return { url: location.href, title: document.title,
           text: document.body ? document.body.innerText : '', elements: out };
})()"#;

/// Finds an element by number or selector, scrolls it into view, says where it is and
/// whether it's an owner-only field.
pub(super) const LOCATE_JS: &str = r#"((t) => {
  const el = /^\d+$/.test(t) ? document.querySelector(`[data-zoen-ref="${t}"]`)
                             : document.querySelector(t);
  if (!el) return null;
  el.scrollIntoView({block: 'center', inline: 'center'});
  const r = el.getBoundingClientRect();
  const ty = (el.type || '').toLowerCase();
  const ac = (el.autocomplete || '').toLowerCase();
  const n = ((el.name || '') + ' ' + (el.id || '')).toLowerCase();
  const secret = ty === 'password' || ac.includes('one-time-code') || ac.includes('password') ||
    ac.startsWith('cc-') || /otp|totp|2fa|mfa|one.?time|passcode|\bpin\b|cvc|cvv/.test(n);
  return { x: r.left + r.width / 2, y: r.top + r.height / 2, secret };
})"#;
