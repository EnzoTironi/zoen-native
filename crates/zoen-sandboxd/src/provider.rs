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
use zoen_agentd::Tier;
use zoen_egress::{Egress, LeasePolicy};
use zoen_guestd::{Request, Response, EGRESS_PORT};

/// The hardware a template is built for; a snapshot can only restore into the same shape.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct Shape {
    pub vcpu: u8,
    pub mem_mib: u32,
    pub work_mib: u32,
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
        }
    }

    fn key(&self) -> String {
        format!("{}c-{}m-{}w", self.vcpu, self.mem_mib, self.work_mib)
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

impl FirecrackerProvider {
    /// Kills VMs a previous process left under `cfg.work`, then starts empty.
    pub fn new(cfg: FirecrackerConfig, egress: Option<Arc<Egress>>) -> Arc<Self> {
        jail::sweep(&cfg);
        let _ = std::fs::remove_dir_all(cfg.work.join("suspended"));
        Arc::new_cyclic(|me| FirecrackerProvider {
            cfg,
            egress,
            inner: Mutex::new(Inner::default()),
            building: tokio::sync::Mutex::new(()),
            me: me.clone(),
        })
    }

    pub fn last_acquire(&self) -> AcquireTiming {
        self.inner.lock().unwrap().last_acquire
    }

    pub fn pooled(&self, shape: Shape) -> usize {
        self.inner
            .lock()
            .unwrap()
            .pool
            .get(&shape)
            .map_or(0, |q| q.len())
    }

    /// The cgroup a running lease's VMM lives in (to read `memory.peak`, `cpu.stat`, ...).
    pub fn cgroup_of(&self, lease: &Lease) -> Option<PathBuf> {
        self.inner
            .lock()
            .unwrap()
            .running
            .get(&lease.id)
            .map(|r| r.vm.cgroup.clone())
    }

    pub fn limits_of(&self, lease: &Lease) -> Option<Limits> {
        self.inner
            .lock()
            .unwrap()
            .running
            .get(&lease.id)
            .map(|r| r.vm.limits)
    }

    /// Builds the template for `shape` if missing and fills its pool; returns the cold-boot
    /// plus snapshot time when it had to build.
    pub async fn warm(&self, shape: Shape) -> Result<Option<Duration>, SandboxError> {
        let started = Instant::now();
        let built = self.template(shape).await?.1;
        self.refill(shape).await;
        Ok(built.then(|| started.elapsed()))
    }

    async fn template(&self, shape: Shape) -> Result<(Arc<Template>, bool), SandboxError> {
        if let Some(t) = self.inner.lock().unwrap().templates.get(&shape) {
            return Ok((t.clone(), false));
        }
        let _one = self.building.lock().await;
        if let Some(t) = self.inner.lock().unwrap().templates.get(&shape) {
            return Ok((t.clone(), false));
        }
        let t = Arc::new(self.build_template(shape).await?);
        self.inner
            .lock()
            .unwrap()
            .templates
            .insert(shape, t.clone());
        Ok((t, true))
    }

    async fn build_template(&self, shape: Shape) -> Result<Template, SandboxError> {
        let limits = Limits::for_shape(&self.cfg, shape.vcpu, shape.mem_mib);
        let vm = Vm::launch(
            &self.cfg,
            limits,
            &[
                (&self.cfg.kernel, "vmlinux"),
                (&self.cfg.rootfs, "rootfs.ext4"),
            ],
        )
        .await
        .map_err(backend)?;
        let r = self.boot_and_snapshot(&vm, shape).await;
        let out = match r {
            Ok(()) => {
                let dir = self.cfg.work.join("templates").join(shape.key());
                let _ = tokio::fs::remove_dir_all(&dir).await;
                tokio::fs::create_dir_all(&dir).await.map_err(backend)?;
                let state = dir.join("snap.state");
                let mem = dir.join("snap.mem");
                let moved = async {
                    tokio::fs::rename(vm.root.join("snap.state"), &state).await?;
                    tokio::fs::rename(vm.root.join("snap.mem"), &mem).await
                }
                .await;
                moved.map(|_| Template { state, mem }).map_err(backend)
            }
            Err(e) => {
                let log = tokio::fs::read_to_string(vm.root.join("fc.log"))
                    .await
                    .unwrap_or_default();
                Err(backend(format!(
                    "template build: {e}; firecracker log: {log}"
                )))
            }
        };
        vm.kill().await;
        out
    }

    async fn boot_and_snapshot(&self, vm: &Vm, shape: Shape) -> Result<(), SandboxError> {
        let sock = vm.api();
        let boot_args = format!(
            "console=ttyS0 reboot=k panic=1 pci=off quiet init=/sbin/zoen-guestd zoen.work_mib={}",
            shape.work_mib
        );
        let calls: [(&'static str, &str, serde_json::Value); 5] = [
            (
                "PUT",
                "/boot-source",
                json!({"kernel_image_path": "/vmlinux", "boot_args": boot_args}),
            ),
            (
                "PUT",
                "/drives/rootfs",
                json!({"drive_id": "rootfs", "path_on_host": "/rootfs.ext4",
                       "is_root_device": true, "is_read_only": true}),
            ),
            (
                "PUT",
                "/machine-config",
                json!({"vcpu_count": shape.vcpu, "mem_size_mib": shape.mem_mib}),
            ),
            (
                "PUT",
                "/vsock",
                json!({"guest_cid": 3, "uds_path": "/v.sock"}),
            ),
            ("PUT", "/entropy", json!({})),
        ];
        for (m, p, b) in calls {
            api::call(&sock, m, p, &b).await.map_err(backend)?;
        }
        api::call(
            &sock,
            "PUT",
            "/actions",
            &json!({"action_type": "InstanceStart"}),
        )
        .await
        .map_err(backend)?;
        // Wait for zoen-guestd.
        let deadline = Instant::now() + self.cfg.boot_timeout;
        loop {
            let ping = Request::Exec {
                argv: vec!["/bin/true".into()],
                env: vec![],
                cwd: Some("/".into()),
                stdin_b64: None,
                timeout_ms: 1000,
                max_output: 0,
            };
            if let Ok(r) = vsock::request(&vm.vsock(), &ping, Duration::from_secs(2)).await {
                if r.ok {
                    break;
                }
            }
            if Instant::now() > deadline {
                return Err(backend("guest agent did not come up"));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        api::call(&sock, "PATCH", "/vm", &json!({"state": "Paused"}))
            .await
            .map_err(backend)?;
        api::call(
            &sock,
            "PUT",
            "/snapshot/create",
            &json!({"snapshot_type": "Full", "snapshot_path": "/snap.state",
                    "mem_file_path": "/snap.mem"}),
        )
        .await
        .map_err(backend)
    }

    /// A paused VM restored from `state` and `mem`.
    async fn restore(&self, shape: Shape, state: &Path, mem: &Path) -> Result<Vm, SandboxError> {
        let limits = Limits::for_shape(&self.cfg, shape.vcpu, shape.mem_mib);
        let vm = Vm::launch(
            &self.cfg,
            limits,
            &[
                (&self.cfg.rootfs, "rootfs.ext4"),
                (state, "snap.state"),
                (mem, "snap.mem"),
            ],
        )
        .await
        .map_err(backend)?;
        let r = api::call(
            &vm.api(),
            "PUT",
            "/snapshot/load",
            &json!({"snapshot_path": "/snap.state",
                    "mem_backend": {"backend_type": "File", "backend_path": "/snap.mem"},
                    "resume_vm": false}),
        )
        .await;
        if let Err(e) = r {
            vm.kill().await;
            return Err(backend(e));
        }
        Ok(vm)
    }

    async fn refill(&self, shape: Shape) {
        let Ok((t, _)) = self.template(shape).await else {
            return;
        };
        while self.pooled(shape) < self.cfg.pool_size {
            match self.restore(shape, &t.state, &t.mem).await {
                Ok(vm) => self
                    .inner
                    .lock()
                    .unwrap()
                    .pool
                    .entry(shape)
                    .or_default()
                    .push_back(vm),
                Err(_) => break,
            }
        }
    }

    fn spawn_refill(&self, shape: Shape) {
        if let Some(me) = self.me.upgrade() {
            tokio::spawn(async move { me.refill(shape).await });
        }
    }

    /// Listens on the VM's vsock port for egress (`v.sock_1080` in its chroot) and hands each
    /// connection to the egress proxy as this lease.
    fn bridge(&self, vm: &Vm, lease: &str) -> Result<Option<JoinHandle<()>>, SandboxError> {
        let Some(egress) = self.egress.clone() else {
            return Ok(None);
        };
        let _ = std::fs::remove_file(vm.root.join(format!("v.sock_{EGRESS_PORT}")));
        let path = vm.sock_path(&format!("v.sock_{EGRESS_PORT}"));
        let listener = UnixListener::bind(&path).map_err(backend)?;
        let lease = lease.to_string();
        Ok(Some(tokio::spawn(async move {
            while let Ok((conn, _)) = listener.accept().await {
                let egress = egress.clone();
                let lease = lease.clone();
                tokio::spawn(async move {
                    let _ = egress.serve_stream(conn, &lease).await;
                });
            }
        })))
    }

    async fn hello(&self, vm: &Vm, ca_pem: Option<String>) -> Result<(), SandboxError> {
        let mut entropy = [0u8; 64];
        getrandom::getrandom(&mut entropy).expect("os randomness");
        let r = vsock::request(
            &vm.vsock(),
            &Request::Hello {
                entropy_b64: B64.encode(entropy),
                now_ms: now_ms(),
                ca_pem,
            },
            Duration::from_secs(10),
        )
        .await
        .map_err(backend)?;
        if !r.ok {
            return Err(backend(r.error.unwrap_or_default()));
        }
        Ok(())
    }

    /// Resumes `vm` for `lease`: egress bridge first, then the vCPUs, then hello.
    async fn start_for(
        &self,
        vm: &Vm,
        lease: &str,
        ca_pem: Option<String>,
    ) -> Result<Option<JoinHandle<()>>, SandboxError> {
        let bridge = self.bridge(vm, lease)?;
        let r = async {
            api::call(&vm.api(), "PATCH", "/vm", &json!({"state": "Resumed"}))
                .await
                .map_err(backend)?;
            self.hello(vm, ca_pem).await
        }
        .await;
        match r {
            Ok(()) => Ok(bridge),
            Err(e) => {
                if let Some(b) = bridge {
                    b.abort();
                }
                Err(e)
            }
        }
    }

    fn running(&self, lease: &Lease) -> Result<(PathBuf, u32), SandboxError> {
        let st = self.inner.lock().unwrap();
        let r = st.running.get(&lease.id).ok_or(SandboxError::NoLease)?;
        Ok((r.vm.vsock(), r.spec.max_secs))
    }

    async fn guest(
        &self,
        lease: &Lease,
        req: Request,
        limit: Duration,
    ) -> Result<Response, SandboxError> {
        let (uds, _) = self.running(lease)?;
        let r = vsock::request(&uds, &req, limit).await.map_err(backend)?;
        if !r.ok {
            return Err(backend(r.error.clone().unwrap_or_default()));
        }
        Ok(r)
    }

    fn policy(&self, lease: &Lease, spec: &SandboxSpec) -> LeasePolicy {
        LeasePolicy {
            lease: lease.id.clone(),
            // Never given to the guest: its connections arrive pre-authenticated over vsock.
            token: random_hex(16),
            owner: lease.owner.clone(),
            agent: spec.agent.clone().unwrap_or_else(|| lease.owner.clone()),
            tool: spec.tool.clone(),
            rules: spec.egress.clone(),
            secrets: spec.secrets.clone(),
            sign_requests: false,
            browser: false,
        }
    }
}

impl Drop for FirecrackerProvider {
    fn drop(&mut self) {
        let st = self.inner.get_mut().unwrap();
        for q in st.pool.values() {
            for vm in q {
                vm.kill_blocking();
            }
        }
        for r in st.running.values() {
            if let Some(b) = &r.bridge {
                b.abort();
            }
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
        if spec.tier != Tier::MicroVm {
            return Err(SandboxError::Unsupported("run this tier (microVM only)"));
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
        let ca = self.egress.as_ref().and_then(|e| e.lease_ca_pem(&lease.id));
        let bridge = match self.start_for(&vm, &lease.id, ca).await {
            Ok(b) => b,
            Err(e) => {
                vm.kill().await;
                if let Some(eg) = &self.egress {
                    eg.revoke(&lease.id);
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
        if let Some(b) = &r.bridge {
            b.abort();
        }
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
                eg.revoke(&lease.id);
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
            if let Some(b) = r.bridge {
                b.abort();
            }
            r.vm.kill().await;
        }
        if let Some(s) = s {
            let _ = tokio::fs::remove_dir_all(&s.dir).await;
        }
        if let Some(e) = &self.egress {
            e.revoke(&lease.id);
        }
        Ok(())
    }
}
