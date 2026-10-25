//! [`FirecrackerProvider`]: the microVM tier behind `zoen_agentd::SandboxProvider`.
//!
//! - **Templates.** The first lease of a shape (vCPUs, memory, work space) boots one VM from
//!   the kernel and the guest root filesystem, waits for `zoen-guestd`, pauses it and takes a
//!   full snapshot. That is the only cold boot.
//! - **Pool.** VMs are restored from the template snapshot, paused, and kept warm; acquiring
//!   one resumes it. The pool refills in the background. Each restored VM is told fresh
//!   entropy and the clock first (restore-once rule, ADR 0028 §3).
//! - **Limits.** Every VMM runs under the jailer in its own cgroup: `memory.max`, `cpu.max`,
//!   `pids.max` ([`Limits`]).
//! - **Network.** The VM has no NIC. Its only way out is the in-guest proxy address, carried
//!   over vsock to this process, which hands each connection to [`zoen_egress::Egress`]
//!   already bound to the lease.
//! - **Browsers** (T2). A browser shape boots the browser image, and its template is taken
//!   with Chromium already running and trusting the node's sandbox root, so a lease starts
//!   from a warm browser. The agent's tools ([`BrowserTools`]) and the owner's live view and
//!   takeover ([`LiveView`]) are calls into the VM; frames arrive sealed to the owner's
//!   device and are relayed as they are.
//! - **Suspend/resume.** Full snapshot of the lease's own VM, which is then killed; resume
//!   restores it once and deletes the snapshot.

use crate::api;
use crate::jail::{self, Limits, Vm};
use crate::vsock;
use crate::FirecrackerConfig;
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use bytes::Bytes;
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::UnixListener;
use tokio::task::JoinHandle;
use zoen_agentd::sandbox::{
    ExecOutput, ExecRequest, Lease, SandboxError, SandboxProvider, SandboxSpec, SnapshotRef,
    MAX_OUTPUT,
};
use zoen_agentd::{BrowserCall, BrowserTools, Tier};
use zoen_egress::{Egress, LeasePolicy};
use zoen_guestd::{BrowserOp, Request, Response, EGRESS_PORT, LIVE_PORT};

/// The hardware a template is built for; a snapshot can only restore into the same shape.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct Shape {
    pub vcpu: u8,
    pub mem_mib: u32,
    pub work_mib: u32,
    /// The browser image, with Chromium running in the template.
    pub browser: bool,
}

impl Shape {
    pub fn of(spec: &SandboxSpec) -> Self {
        let vcpu = spec.vcpu.max(1);
        let mem_mib = spec.mem_mib.max(128);
        // /work is a tmpfs inside the VM's memory in P1 (so it survives suspend); a separate
        // writable disk per lease comes with the template builder.
        let work_mib = spec.disk_mib.clamp(16, mem_mib / 2);
        Shape {
            vcpu,
            mem_mib,
            work_mib,
            browser: spec.tier == Tier::Browser,
        }
    }

    fn key(&self) -> String {
        format!(
            "{}{}c-{}m-{}w",
            if self.browser { "b" } else { "" },
            self.vcpu,
            self.mem_mib,
            self.work_mib
        )
    }
}

/// Timings of the last acquire, for measurement and tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct AcquireTiming {
    pub from_pool: bool,
    pub total: Duration,
}

struct Template {
    state: PathBuf,
    mem: PathBuf,
}

struct Running {
    lease: Lease,
    spec: SandboxSpec,
    shape: Shape,
    vm: Vm,
    bridge: Option<JoinHandle<()>>,
    /// Relays live-view frames (ciphertext) while the owner watches.
    live: Option<JoinHandle<()>>,
}

impl Running {
    fn stop_tasks(&self) {
        for t in [&self.bridge, &self.live].into_iter().flatten() {
            t.abort();
        }
    }
}

/// The owner's side of a browser lease: the session offer for their device and the sealed
/// frames, exactly as the VM sent them.
pub struct LiveView {
    pub offer: zoen_liveview::Offer,
    pub frames: tokio::sync::mpsc::Receiver<Vec<u8>>,
}

struct Suspended {
    lease: Lease,
    spec: SandboxSpec,
    shape: Shape,
    dir: PathBuf,
}

#[derive(Default)]
struct Inner {
    templates: HashMap<Shape, Arc<Template>>,
    pool: HashMap<Shape, VecDeque<Vm>>,
    running: HashMap<String, Running>,
    suspended: HashMap<String, Suspended>,
    last_acquire: AcquireTiming,
}

pub struct FirecrackerProvider {
    cfg: FirecrackerConfig,
    egress: Option<Arc<Egress>>,
    inner: Mutex<Inner>,
    /// One template build per shape at a time.
    building: tokio::sync::Mutex<()>,
    me: Weak<FirecrackerProvider>,
}

fn backend(e: impl std::fmt::Display) -> SandboxError {
    SandboxError::Backend(e.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    getrandom::getrandom(&mut b).expect("os randomness");
    hex::encode(b)
}

mod pool;

impl Drop for FirecrackerProvider {
    fn drop(&mut self) {
        let st = self.inner.get_mut().unwrap();
        for q in st.pool.values() {
            for vm in q {
                vm.kill_blocking();
            }
        }
        for r in st.running.values() {
            r.stop_tasks();
            r.vm.kill_blocking();
        }
        for s in st.suspended.values() {
            let _ = std::fs::remove_dir_all(&s.dir);
        }
    }
}

fn b64(s: &str) -> Bytes {
    Bytes::from(B64.decode(s).unwrap_or_default())
}

#[async_trait]
impl SandboxProvider for FirecrackerProvider {
    async fn acquire(&self, spec: &SandboxSpec, owner: &str) -> Result<Lease, SandboxError> {
        match spec.tier {
            Tier::MicroVm => {}
            Tier::Browser if self.cfg.browser_rootfs.is_some() => {}
            Tier::Browser => {
                return Err(SandboxError::Unsupported("run browsers (no browser image)"))
            }
            _ => return Err(SandboxError::Unsupported("run this tier")),
        }
        let started = Instant::now();
        let shape = Shape::of(spec);
        let pooled = self
            .inner
            .lock()
            .unwrap()
            .pool
            .get_mut(&shape)
            .and_then(|q| q.pop_front());
        let from_pool = pooled.is_some();
        let vm = match pooled {
            Some(vm) => vm,
            None => {
                let (t, _) = self.template(shape).await?;
                self.restore(shape, &t.state, &t.mem).await?
            }
        };
        self.spawn_refill(shape);
        let lease = Lease {
            id: format!("lease_{}", random_hex(12)),
            owner: owner.into(),
            tool: spec.tool.clone(),
            tier: spec.tier,
        };
        if let Some(e) = &self.egress {
            if let Err(err) = e.register(self.policy(&lease, spec)) {
                vm.kill().await;
                return Err(backend(err));
            }
        }
        // The lease CA's certificate (never its key) joins the guest's trust bundle.
        let ca = match self
            .egress
            .as_ref()
            .map(|egress| egress.lease_ca_pem(&lease.id))
            .transpose()
        {
            Ok(certificate) => certificate.flatten(),
            Err(error) => {
                vm.kill().await;
                return Err(backend(error));
            }
        };
        let bridge = match self.start_for(&vm, &lease.id, ca).await {
            Ok(b) => b,
            Err(e) => {
                vm.kill().await;
                if let Some(eg) = &self.egress {
                    eg.revoke(&lease.id)
                        .map_err(|cleanup| backend(format!("{e}; egress cleanup: {cleanup}")))?;
                }
                return Err(e);
            }
        };
        let mut st = self.inner.lock().unwrap();
        st.last_acquire = AcquireTiming {
            from_pool,
            total: started.elapsed(),
        };
        st.running.insert(
            lease.id.clone(),
            Running {
                lease: lease.clone(),
                spec: spec.clone(),
                shape,
                vm,
                bridge,
                live: None,
            },
        );
        Ok(lease)
    }

    async fn exec(&self, lease: &Lease, req: ExecRequest) -> Result<ExecOutput, SandboxError> {
        let (_, max_secs) = self.running(lease)?;
        let timeout = req
            .timeout
            .min(Duration::from_secs(u64::from(max_secs.max(1))));
        let started = Instant::now();
        let r = self
            .guest(
                lease,
                Request::Exec {
                    argv: req.argv,
                    env: req.env,
                    cwd: req.cwd,
                    stdin_b64: req.stdin.map(|b| B64.encode(b)),
                    timeout_ms: timeout.as_millis() as u64,
                    max_output: MAX_OUTPUT,
                },
                timeout + Duration::from_secs(5),
            )
            .await?;
        Ok(ExecOutput {
            exit_code: r.exit_code,
            stdout: b64(&r.stdout_b64),
            stderr: b64(&r.stderr_b64),
            timed_out: r.timed_out,
            elapsed: started.elapsed(),
        })
    }

    async fn put_file(&self, lease: &Lease, path: &str, bytes: Bytes) -> Result<(), SandboxError> {
        self.guest(
            lease,
            Request::Put {
                path: path.into(),
                data_b64: B64.encode(bytes),
            },
            Duration::from_secs(60),
        )
        .await
        .map(|_| ())
    }

    async fn get_file(&self, lease: &Lease, path: &str) -> Result<Bytes, SandboxError> {
        self.guest(
            lease,
            Request::Get {
                path: path.into(),
                max_bytes: 64 * 1024 * 1024,
            },
            Duration::from_secs(60),
        )
        .await
        .map(|r| b64(&r.data_b64))
    }

    async fn suspend(&self, lease: &Lease) -> Result<SnapshotRef, SandboxError> {
        let r = self
            .inner
            .lock()
            .unwrap()
            .running
            .remove(&lease.id)
            .ok_or(SandboxError::NoLease)?;
        let sock = r.vm.api();
        let res = async {
            api::call(&sock, "PATCH", "/vm", &json!({"state": "Paused"})).await?;
            api::call(
                &sock,
                "PUT",
                "/snapshot/create",
                &json!({"snapshot_type": "Full", "snapshot_path": "/susp.state",
                        "mem_file_path": "/susp.mem"}),
            )
            .await
        }
        .await;
        r.stop_tasks();
        let dir = self.cfg.work.join("suspended").join(&lease.id);
        let moved = match res {
            Ok(()) => async {
                tokio::fs::create_dir_all(&dir).await?;
                tokio::fs::rename(r.vm.root.join("susp.state"), dir.join("snap.state")).await?;
                tokio::fs::rename(r.vm.root.join("susp.mem"), dir.join("snap.mem")).await
            }
            .await
            .map_err(backend),
            Err(e) => Err(backend(e)),
        };
        r.vm.kill().await;
        if let Err(e) = moved {
            let _ = tokio::fs::remove_dir_all(&dir).await;
            if let Some(eg) = &self.egress {
                eg.revoke(&lease.id)
                    .map_err(|cleanup| backend(format!("{e}; egress cleanup: {cleanup}")))?;
            }
            return Err(e);
        }
        let snap = SnapshotRef {
            id: format!("snap_{}", random_hex(8)),
            lease: lease.id.clone(),
        };
        self.inner.lock().unwrap().suspended.insert(
            lease.id.clone(),
            Suspended {
                lease: r.lease,
                spec: r.spec,
                shape: r.shape,
                dir,
            },
        );
        Ok(snap)
    }

    async fn resume(&self, snap: &SnapshotRef) -> Result<Lease, SandboxError> {
        let s = self
            .inner
            .lock()
            .unwrap()
            .suspended
            .remove(&snap.lease)
            .ok_or(SandboxError::NoLease)?;
        let vm = self
            .restore(s.shape, &s.dir.join("snap.state"), &s.dir.join("snap.mem"))
            .await;
        // Restore once: the snapshot is gone whether or not the restore worked.
        let _ = tokio::fs::remove_dir_all(&s.dir).await;
        let vm = vm?;
        let bridge = match self.start_for(&vm, &s.lease.id, None).await {
            Ok(b) => b,
            Err(e) => {
                vm.kill().await;
                return Err(e);
            }
        };
        let lease = s.lease.clone();
        self.inner.lock().unwrap().running.insert(
            lease.id.clone(),
            Running {
                lease: s.lease,
                spec: s.spec,
                shape: s.shape,
                vm,
                bridge,
                live: None,
            },
        );
        Ok(lease)
    }

    async fn release(&self, lease: Lease) -> Result<(), SandboxError> {
        let (r, s) = {
            let mut st = self.inner.lock().unwrap();
            (st.running.remove(&lease.id), st.suspended.remove(&lease.id))
        };
        if let Some(r) = r {
            r.stop_tasks();
            r.vm.kill().await;
        }
        if let Some(s) = s {
            let _ = tokio::fs::remove_dir_all(&s.dir).await;
        }
        if let Some(e) = &self.egress {
            e.revoke(&lease.id).map_err(backend)?;
        }
        Ok(())
    }
}

#[async_trait]
impl BrowserTools for FirecrackerProvider {
    async fn browser(&self, lease: &Lease, call: BrowserCall) -> Result<String, SandboxError> {
        let (_, max_secs) = self.running(lease)?;
        let timeout_ms = (u64::from(max_secs.max(1)) * 1000).min(60_000);
        let limit = Duration::from_millis(timeout_ms + 10_000);
        let page = |v: &serde_json::Value| {
            format!(
                "{} ({})",
                v["title"].as_str().unwrap_or(""),
                v["url"].as_str().unwrap_or("")
            )
        };
        match call {
            BrowserCall::Open { url } => {
                let v = self
                    .browser_op(lease, BrowserOp::Open { url, timeout_ms }, limit)
                    .await?;
                Ok(format!("Opened {}", page(&v)))
            }
            BrowserCall::Read => {
                let v = self
                    .browser_op(lease, BrowserOp::Read { max_chars: 20_000 }, limit)
                    .await?;
                Ok(v["text"].as_str().unwrap_or("").to_string())
            }
            BrowserCall::Click { target } => {
                let v = self
                    .browser_op(lease, BrowserOp::Click { target, timeout_ms }, limit)
                    .await?;
                Ok(format!("Clicked; now on {}", page(&v)))
            }
            BrowserCall::Type {
                target,
                text,
                submit,
            } => {
                let v = self
                    .browser_op(
                        lease,
                        BrowserOp::Type {
                            target,
                            text,
                            submit,
                            timeout_ms,
                        },
                        limit,
                    )
                    .await?;
                Ok(format!("Typed; now on {}", page(&v)))
            }
        }
    }
}
