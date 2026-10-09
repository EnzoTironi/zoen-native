//! The sandbox interface (ADR 0028 §4). Backends: [`fake`] for unit tests, [`gvisor`] as the
//! P0 stand-in (and the no-KVM fallback), Firecracker nodes in P1, Fly Machines for staging.

use crate::router::Tier;
use async_trait::async_trait;
use bytes::Bytes;
use roda_types::IdentityId;
use std::time::Duration;
use zoen_egress::{EgressRule, SecretBinding};

pub mod fake;
pub mod gvisor;

/// What to start: the manifest's needs plus the template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SandboxSpec {
    pub tier: Tier,
    pub tool: String,
    pub template: String,
    pub vcpu: u8,
    pub mem_mib: u32,
    pub disk_mib: u32,
    pub max_secs: u32,
    pub egress: Vec<EgressRule>,
    pub secrets: Vec<SecretBinding>,
    /// The agent the tool works for: approval cards and `net:` Grants name it. `None` when the
    /// owner runs the tool directly.
    pub agent: Option<IdentityId>,
}

/// A running (or suspended) sandbox held for one owner's tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    pub id: String,
    pub owner: IdentityId,
    pub tool: String,
    pub tier: Tier,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotRef {
    pub id: String,
    pub lease: String,
}

#[derive(Clone, Debug, Default)]
pub struct ExecRequest {
    pub argv: Vec<String>,
    pub stdin: Option<Bytes>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<String>,
    pub timeout: Duration,
}

impl ExecRequest {
    pub fn sh(script: &str) -> Self {
        ExecRequest {
            argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
            timeout: Duration::from_secs(30),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecOutput {
    /// `None` when killed (timeout).
    pub exit_code: Option<i32>,
    pub stdout: Bytes,
    pub stderr: Bytes,
    pub timed_out: bool,
    pub elapsed: Duration,
}

impl ExecOutput {
    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("this backend can't {0}")]
    Unsupported(&'static str),
    #[error("no such lease")]
    NoLease,
    #[error("sandbox failed: {0}")]
    Backend(String),
}

/// Most output a single exec returns; the rest is cut (and says so in `stderr`).
pub const MAX_OUTPUT: usize = 1024 * 1024;

#[async_trait]
pub trait SandboxProvider: Send + Sync {
    /// Starts (or restores from a warm pool) a sandbox for `spec`.
    async fn acquire(&self, spec: &SandboxSpec, owner: &str) -> Result<Lease, SandboxError>;
    async fn exec(&self, lease: &Lease, req: ExecRequest) -> Result<ExecOutput, SandboxError>;
    async fn put_file(&self, lease: &Lease, path: &str, bytes: Bytes) -> Result<(), SandboxError>;
    async fn get_file(&self, lease: &Lease, path: &str) -> Result<Bytes, SandboxError>;
    /// Stops paying for CPU and RAM, keeping state. Backends that can't keep a running
    /// process still keep the disk.
    async fn suspend(&self, _lease: &Lease) -> Result<SnapshotRef, SandboxError> {
        Err(SandboxError::Unsupported("suspend"))
    }
    async fn resume(&self, _snap: &SnapshotRef) -> Result<Lease, SandboxError> {
        Err(SandboxError::Unsupported("resume"))
    }
    async fn fork(&self, _lease: &Lease) -> Result<Lease, SandboxError> {
        Err(SandboxError::Unsupported("fork"))
    }
    /// Tears down: processes, disk and snapshots go.
    async fn release(&self, lease: Lease) -> Result<(), SandboxError>;
}

pub(crate) fn new_lease_id() -> String {
    let mut b = [0u8; 12];
    getrandom::getrandom(&mut b).expect("os randomness");
    format!("lease_{}", hex::encode(b))
}
