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
}

impl Response {
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
