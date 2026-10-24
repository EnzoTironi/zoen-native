//! VM templates, warm pool and lease-bound guest operations.
use super::{
    api, backend, jail, now_ms, random_hex, vsock, AcquireTiming, Arc, BrowserOp, Duration, Egress,
    FirecrackerConfig, FirecrackerProvider, Inner, Instant, JoinHandle, Lease, LeasePolicy, Limits,
    LiveView, Mutex, Path, PathBuf, Request, Response, SandboxError, SandboxSpec, Shape, Template,
    Tier, UnixListener, Vm, B64, EGRESS_PORT, LIVE_PORT,
};
use base64::Engine;
use serde_json::json;

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

    pub(super) async fn template(
        &self,
        shape: Shape,
    ) -> Result<(Arc<Template>, bool), SandboxError> {
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

    pub(super) fn rootfs(&self, shape: Shape) -> Result<&Path, SandboxError> {
        if shape.browser {
            self.cfg
                .browser_rootfs
                .as_deref()
                .ok_or(SandboxError::Unsupported("run browsers (no browser image)"))
        } else {
            Ok(&self.cfg.rootfs)
        }
    }

    pub(super) fn pool_size(&self, shape: Shape) -> usize {
        if shape.browser {
            self.cfg.browser_pool_size
        } else {
            self.cfg.pool_size
        }
    }

    pub(super) async fn build_template(&self, shape: Shape) -> Result<Template, SandboxError> {
        let limits = Limits::for_shape(&self.cfg, shape.vcpu, shape.mem_mib);
        let vm = Vm::launch(
            &self.cfg,
            limits,
            &[
                (&self.cfg.kernel, "vmlinux"),
                (self.rootfs(shape)?, "rootfs.ext4"),
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

    pub(super) async fn boot_and_snapshot(
        &self,
        vm: &Vm,
        shape: Shape,
    ) -> Result<(), SandboxError> {
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
        if shape.browser {
            // Chromium starts here, once per template, trusting the node's sandbox root
            // (browser leases' CAs are issued under it).
            let root_ca_pem = self.egress.as_ref().map(|e| e.sandbox_root_pem());
            let r = vsock::request(
                &vm.vsock(),
                &Request::Browser {
                    browser: BrowserOp::Launch { root_ca_pem },
                },
                Duration::from_secs(90),
            )
            .await
            .map_err(backend)?;
            if !r.ok {
                return Err(backend(format!(
                    "browser launch: {}",
                    r.error.unwrap_or_default()
                )));
            }
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
    pub(super) async fn restore(
        &self,
        shape: Shape,
        state: &Path,
        mem: &Path,
    ) -> Result<Vm, SandboxError> {
        let limits = Limits::for_shape(&self.cfg, shape.vcpu, shape.mem_mib);
        let vm = Vm::launch(
            &self.cfg,
            limits,
            &[
                (self.rootfs(shape)?, "rootfs.ext4"),
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

    pub(super) async fn refill(&self, shape: Shape) {
        let Ok((t, _)) = self.template(shape).await else {
            return;
        };
        while self.pooled(shape) < self.pool_size(shape) {
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

    pub(super) fn spawn_refill(&self, shape: Shape) {
        if let Some(me) = self.me.upgrade() {
            tokio::spawn(async move { me.refill(shape).await });
        }
    }

    /// Listens on the VM's vsock port for egress (`v.sock_1080` in its chroot) and hands each
    /// connection to the egress proxy as this lease.
    pub(super) fn bridge(
        &self,
        vm: &Vm,
        lease: &str,
    ) -> Result<Option<JoinHandle<()>>, SandboxError> {
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

    pub(super) async fn hello(&self, vm: &Vm, ca_pem: Option<String>) -> Result<(), SandboxError> {
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
    pub(super) async fn start_for(
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

    pub(super) fn running(&self, lease: &Lease) -> Result<(PathBuf, u32), SandboxError> {
        let st = self.inner.lock().unwrap();
        let r = st.running.get(&lease.id).ok_or(SandboxError::NoLease)?;
        Ok((r.vm.vsock(), r.spec.max_secs))
    }

    pub(super) async fn guest(
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

    /// A free Unix socket in the VM's chroot that the guest reaches on `port`.
    pub(super) fn listen_on(&self, vm: &Vm, port: u32) -> Result<UnixListener, SandboxError> {
        let _ = std::fs::remove_file(vm.root.join(format!("v.sock_{port}")));
        UnixListener::bind(vm.sock_path(&format!("v.sock_{port}"))).map_err(backend)
    }

    /// One operation on a browser lease's browser.
    pub(super) async fn browser_op(
        &self,
        lease: &Lease,
        op: BrowserOp,
        limit: Duration,
    ) -> Result<serde_json::Value, SandboxError> {
        if lease.tier != Tier::Browser {
            return Err(SandboxError::Unsupported("browse in this tier"));
        }
        let r = self
            .guest(lease, Request::Browser { browser: op }, limit)
            .await?;
        Ok(r.value.unwrap_or(serde_json::Value::Null))
    }

    /// The owner starts watching: frames are sealed in the VM to `device_pub` (their
    /// device's X25519 key) and relayed here without being opened.
    pub async fn live_start(
        &self,
        lease: &Lease,
        device_pub: [u8; 32],
    ) -> Result<LiveView, SandboxError> {
        let listener = {
            let st = self.inner.lock().unwrap();
            let r = st.running.get(&lease.id).ok_or(SandboxError::NoLease)?;
            if let Some(old) = &r.live {
                old.abort();
            }
            self.listen_on(&r.vm, LIVE_PORT)?
        };
        let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
        let relay = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let Ok((mut conn, _)) = listener.accept().await else {
                return;
            };
            loop {
                let mut len = [0u8; 4];
                if conn.read_exact(&mut len).await.is_err() {
                    return;
                }
                let n = u32::from_be_bytes(len) as usize;
                if n > 8 << 20 {
                    return;
                }
                let mut frame = vec![0u8; n];
                if conn.read_exact(&mut frame).await.is_err() {
                    return;
                }
                // A viewer that falls behind loses frames, never the browser's time.
                let _ = tx.try_send(frame);
            }
        });
        if let Some(r) = self.inner.lock().unwrap().running.get_mut(&lease.id) {
            r.live = Some(relay);
        }
        let v = self
            .browser_op(
                lease,
                BrowserOp::LiveStart {
                    device_pub_b64: B64.encode(device_pub),
                },
                Duration::from_secs(20),
            )
            .await?;
        let field = |k: &str| -> Result<Vec<u8>, SandboxError> {
            v[k].as_str()
                .and_then(|s| B64.decode(s).ok())
                .ok_or_else(|| backend("bad live-view offer"))
        };
        let offer = zoen_liveview::Offer {
            vm_pub: field("vm_pub_b64")?
                .try_into()
                .map_err(|_| backend("bad live-view key"))?,
            session: field("session_b64")?
                .try_into()
                .map_err(|_| backend("bad live-view session"))?,
        };
        Ok(LiveView { offer, frames: rx })
    }

    pub async fn live_stop(&self, lease: &Lease) -> Result<(), SandboxError> {
        self.browser_op(lease, BrowserOp::LiveStop, Duration::from_secs(10))
            .await?;
        if let Some(r) = self.inner.lock().unwrap().running.get_mut(&lease.id) {
            if let Some(t) = r.live.take() {
                t.abort();
            }
        }
        Ok(())
    }

    /// The owner takes the browser (needs a live view). From now until their device sends a
    /// sealed `Done`, the agent's browser calls, exec and file access are refused in the VM.
    pub async fn takeover_begin(&self, lease: &Lease) -> Result<(), SandboxError> {
        self.browser_op(lease, BrowserOp::TakeoverBegin, Duration::from_secs(10))
            .await
            .map(|_| ())
    }

    /// Relays one sealed event from the owner's device. Returns whether it ended the takeover.
    pub async fn takeover_input(&self, lease: &Lease, sealed: &[u8]) -> Result<bool, SandboxError> {
        let v = self
            .browser_op(
                lease,
                BrowserOp::TakeoverInput {
                    sealed_b64: B64.encode(sealed),
                },
                Duration::from_secs(30),
            )
            .await?;
        Ok(v["done"].as_bool().unwrap_or(false))
    }

    pub async fn browser_status(&self, lease: &Lease) -> Result<serde_json::Value, SandboxError> {
        self.browser_op(lease, BrowserOp::Status, Duration::from_secs(10))
            .await
    }

    pub(super) fn policy(&self, lease: &Lease, spec: &SandboxSpec) -> LeasePolicy {
        LeasePolicy {
            lease: lease.id.clone(),
            // Never given to the guest: its connections arrive pre-authenticated over vsock.
            token: random_hex(16),
            owner: lease.owner.clone(),
            agent: spec.agent.clone().unwrap_or_else(|| lease.owner.clone()),
            tool: spec.tool.clone(),
            rules: spec.egress.clone(),
            secrets: spec.secrets.clone(),
            // Browsers identify as Zoen's signed agent (Web Bot Auth) when the node has a key.
            sign_requests: spec.tier == Tier::Browser,
            browser: spec.tier == Tier::Browser,
        }
    }
}
