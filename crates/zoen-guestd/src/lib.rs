//! The wire protocol between `zoen-sandboxd` (host) and `zoen-guestd` (guest), over vsock.
//!
//! The host connects to the VM's vsock socket on [`CONTROL_PORT`]; each connection carries one
//! JSON request line and gets one JSON response line. Bytes travel as base64. The guest opens
//! connections the other way only on [`EGRESS_PORT`], one per TCP connection a tool makes to
//! the in-VM proxy address [`GUEST_PROXY`]; the host hands each to the egress proxy, already
//! tied to the lease, so the guest never holds a proxy credential.

use serde::{Deserialize, Serialize};

pub const CONTROL_PORT: u32 = 52;
pub const EGRESS_PORT: u32 = 1080;
/// The guest connects to the host on this port to stream live-view frames, already
/// encrypted to the owner's device ([`zoen_liveview`]); the host relays ciphertext only.
pub const LIVE_PORT: u32 = 1081;
/// Where tools inside the VM find the proxy (`HTTP(S)_PROXY`).
pub const GUEST_PROXY: &str = "127.0.0.1:3128";
/// The per-lease CA certificate, written by the host at acquire time.
pub const GUEST_CA_PATH: &str = "/run/zoen/ca.crt";
/// The trust bundle tools use: the image's roots plus the lease CA.
pub const GUEST_TRUST_BUNDLE: &str = "/run/zoen/ca-bundle.crt";
/// Writable work area (tmpfs; part of the VM's memory, so it survives suspend and resume).
pub const GUEST_WORK: &str = "/work";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Sent right after a VM is taken from the pool or resumed: fresh entropy (a restored
    /// snapshot repeats its RNG state otherwise), the wall clock, and the lease CA.
    Hello {
        entropy_b64: String,
        now_ms: i64,
        ca_pem: Option<String>,
    },
    Exec {
        argv: Vec<String>,
        env: Vec<(String, String)>,
        cwd: Option<String>,
        stdin_b64: Option<String>,
        timeout_ms: u64,
        max_output: usize,
    },
    Put {
        path: String,
        data_b64: String,
    },
    Get {
        path: String,
        max_bytes: usize,
    },
    /// The browser in a browser microVM (ADR 0028 §7). CDP never leaves the VM: Chromium
    /// talks to `zoen-guestd` over a pipe, and only these operations cross vsock.
    Browser {
        browser: BrowserOp,
    },
}

/// What can be asked of the in-VM browser. `Open`, `Read`, `Click` and `Type` are the agent's
/// (the model's) tools; the live view and takeover are the owner's, and their input arrives
/// sealed by the owner's device.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BrowserOp {
    /// Starts Chromium (template build time), trusting `root_ca_pem` (the node's sandbox root).
    Launch {
        root_ca_pem: Option<String>,
    },
    Open {
        url: String,
        timeout_ms: u64,
    },
    /// The page as text plus its interactive elements, numbered for `Click` and `Type`.
    /// Password and one-time-code fields show as `[hidden]`.
    Read {
        max_chars: usize,
    },
    /// `target`: an element number from `Read`, or a CSS selector.
    Click {
        target: String,
        timeout_ms: u64,
    },
    /// Refused for password and one-time-code fields: those are the owner's, in a takeover.
    Type {
        target: String,
        text: String,
        submit: bool,
        timeout_ms: u64,
    },
    /// Starts streaming frames, sealed to the device key, to the host on [`LIVE_PORT`].
    LiveStart {
        device_pub_b64: String,
    },
    LiveStop,
    /// The owner takes the browser. The agent's operations (and exec/files) are refused
    /// until the device sends a sealed `Done`.
    TakeoverBegin,
    /// One sealed event from the owner's device.
    TakeoverInput {
        sealed_b64: String,
    },
    Status,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Response {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub timed_out: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stdout_b64: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stderr_b64: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub data_b64: String,
    /// Structured results (browser operations).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

/// Error codes the browser returns as `error` prefixes.
pub const TAKEOVER_IN_PROGRESS: &str = "TAKEOVER_IN_PROGRESS";
pub const FIELD_NEEDS_OWNER: &str = "FIELD_NEEDS_OWNER";

impl Response {
    pub fn value(v: serde_json::Value) -> Self {
        Response {
            ok: true,
            value: Some(v),
            ..Default::default()
        }
    }

    pub fn err(e: impl ToString) -> Self {
        Response {
            ok: false,
            error: Some(e.to_string()),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip() {
        let r = Request::Exec {
            argv: vec!["/bin/true".into()],
            env: vec![("A".into(), "b".into())],
            cwd: None,
            stdin_b64: None,
            timeout_ms: 1000,
            max_output: 10,
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.starts_with("{\"op\":\"exec\""));
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(
            back,
            Request::Exec {
                timeout_ms: 1000,
                ..
            }
        ));
    }
}
