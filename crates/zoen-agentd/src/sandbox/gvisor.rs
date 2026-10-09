//! gVisor (`runsc`) as the P0 stand-in for microVMs, and the fallback where `/dev/kvm` is
//! missing (CI, Fly Machines). Rootless: no root, no daemon.
//!
//! Each lease is a private copy of the template root filesystem. Each exec is a fresh gVisor
//! sandbox over that copy (`runsc run`, network namespace empty, no capabilities,
//! `no_new_privileges`, an address-space limit from the manifest), so files persist between
//! calls and processes don't. Suspend keeps the disk and frees nothing else, because nothing
//! runs between calls. Files go in and out *through* the sandbox (`cat`), never by touching
//! the host copy, so a symlink planted by the tool can't reach host paths.
//!
//! Not enforced here, enforced by Firecracker in P1: vCPU count and the cgroup memory limit
//! (rootless runsc ignores cgroups).

use super::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

type ExecLock = std::sync::Arc<tokio::sync::Mutex<()>>;

pub struct GvisorProvider {
    runsc: PathBuf,
    /// Extracted template root filesystems by name (`"base"` → alpine minirootfs in P0).
    templates: HashMap<String, PathBuf>,
    work: PathBuf,
    leases: Mutex<HashMap<String, (Lease, SandboxSpec, ExecLock)>>,
}

impl GvisorProvider {
    /// `work` holds lease disks and runsc state; it must be private to this process.
    pub fn new(runsc: PathBuf, templates: HashMap<String, PathBuf>, work: PathBuf) -> Self {
        GvisorProvider {
            runsc,
            templates,
            work,
            leases: Mutex::new(HashMap::new()),
        }
    }

    /// From `ZOEN_RUNSC` and `ZOEN_SANDBOX_ROOTFS` (see `scripts/gvisor.sh`).
    pub fn from_env(work: PathBuf) -> Option<Self> {
        let runsc = PathBuf::from(std::env::var_os("ZOEN_RUNSC")?);
        let rootfs = PathBuf::from(std::env::var_os("ZOEN_SANDBOX_ROOTFS")?);
        Some(Self::new(
            runsc,
            HashMap::from([("base".to_string(), rootfs)]),
            work,
        ))
    }

    fn lease_dir(&self, id: &str) -> PathBuf {
        self.work.join(id)
    }

    fn get(&self, lease: &Lease) -> Result<(SandboxSpec, ExecLock), SandboxError> {
        let m = self.leases.lock().unwrap();
        let (_, spec, lock) = m.get(&lease.id).ok_or(SandboxError::NoLease)?;
        Ok((spec.clone(), lock.clone()))
    }

    fn config(spec: &SandboxSpec, req: &ExecRequest) -> serde_json::Value {
        let mut env = vec![
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
            "HOME=/root".to_string(),
        ];
        env.extend(req.env.iter().map(|(k, v)| format!("{k}={v}")));
        let mem = u64::from(spec.mem_mib) * 1024 * 1024;
        serde_json::json!({
            "ociVersion": "1.0.0",
            "process": {
                "user": { "uid": 0, "gid": 0 },
                "args": req.argv,
                "env": env,
                "cwd": req.cwd.clone().unwrap_or_else(|| "/".into()),
                "terminal": false,
                "noNewPrivileges": true,
                "capabilities": { "bounding": [], "effective": [], "inheritable": [], "permitted": [] },
                "rlimits": [
                    { "type": "RLIMIT_NOFILE", "hard": 1024, "soft": 1024 },
                    { "type": "RLIMIT_AS", "hard": mem, "soft": mem },
                    { "type": "RLIMIT_NPROC", "hard": 512, "soft": 512 }
                ]
            },
            "root": { "path": "rootfs", "readonly": false },
            "hostname": "sandbox",
            "mounts": [
                { "destination": "/proc", "type": "proc", "source": "proc" },
                { "destination": "/dev", "type": "tmpfs", "source": "tmpfs" },
                { "destination": "/tmp", "type": "tmpfs", "source": "tmpfs" },
                { "destination": "/sys", "type": "sysfs", "source": "sysfs", "options": ["nosuid", "noexec", "nodev", "ro"] }
            ],
            "linux": {
                "namespaces": [ {"type": "pid"}, {"type": "network"}, {"type": "ipc"}, {"type": "uts"}, {"type": "mount"} ]
            }
        })
    }

    async fn run(&self, lease: &Lease, req: ExecRequest) -> Result<ExecOutput, SandboxError> {
        let (spec, lock) = self.get(lease)?;
        let _one_at_a_time = lock.lock().await;
        let dir = self.lease_dir(&lease.id);
        let bundle = dir.join("bundle");
        tokio::fs::write(
            bundle.join("config.json"),
            Self::config(&spec, &req).to_string(),
        )
        .await
        .map_err(|e| SandboxError::Backend(e.to_string()))?;
        let mut b = [0u8; 6];
        getrandom::getrandom(&mut b).expect("os randomness");
        let cid = format!("{}-{}", lease.id, hex::encode(b));
        let state = dir.join("state");
        let mut cmd = Command::new(&self.runsc);
        cmd.arg("--rootless")
            .arg("--network=none")
            .arg("--overlay2=none")
            .arg("--root")
            .arg(&state)
            .arg("run")
            .arg("--bundle")
            .arg(&bundle)
            .arg(&cid)
            .stdin(if req.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let started = std::time::Instant::now();
        let mut child = cmd
            .spawn()
            .map_err(|e| SandboxError::Backend(format!("runsc: {e}")))?;
        if let Some(input) = req.stdin.clone() {
            let mut si = child.stdin.take().expect("piped");
            tokio::spawn(async move {
                let _ = si.write_all(&input).await;
                let _ = si.shutdown().await;
            });
        }
        let timeout = req
            .timeout
            .min(Duration::from_secs(u64::from(spec.max_secs)));
        match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(out)) => Ok(ExecOutput {
                exit_code: out.status.code(),
                stdout: cap(out.stdout),
                stderr: cap(out.stderr),
                timed_out: false,
                elapsed: started.elapsed(),
            }),
            Ok(Err(e)) => Err(SandboxError::Backend(e.to_string())),
            Err(_) => {
                // The child was dropped (killed); make sure the sandbox is gone too.
                let _ = Command::new(&self.runsc)
                    .arg("--rootless")
                    .arg("--root")
                    .arg(&state)
                    .args(["delete", "--force", &cid])
                    .output()
                    .await;
                Ok(ExecOutput {
                    exit_code: None,
                    timed_out: true,
                    elapsed: started.elapsed(),
                    ..Default::default()
                })
            }
        }
    }
}

fn cap(mut v: Vec<u8>) -> Bytes {
    if v.len() > MAX_OUTPUT {
        v.truncate(MAX_OUTPUT);
        v.extend_from_slice(b"\n[cut at 1 MiB]");
    }
    Bytes::from(v)
}

async fn copy_tree(from: &Path, to: &Path) -> Result<(), SandboxError> {
    let out = Command::new("cp")
        .arg("-a")
        .arg(from)
        .arg(to)
        .output()
        .await
        .map_err(|e| SandboxError::Backend(e.to_string()))?;
    if !out.status.success() {
        return Err(SandboxError::Backend(
            String::from_utf8_lossy(&out.stderr).into(),
        ));
    }
    Ok(())
}

#[async_trait]
impl SandboxProvider for GvisorProvider {
    async fn acquire(&self, spec: &SandboxSpec, owner: &str) -> Result<Lease, SandboxError> {
        if spec.tier == Tier::Browser {
            return Err(SandboxError::Unsupported("run a browser (P2)"));
        }
        let template = self
            .templates
            .get(&spec.template)
            .ok_or_else(|| SandboxError::Backend(format!("no template `{}`", spec.template)))?;
        let lease = Lease {
            id: new_lease_id(),
            owner: owner.into(),
            tool: spec.tool.clone(),
            tier: spec.tier,
        };
        let dir = self.lease_dir(&lease.id);
        tokio::fs::create_dir_all(dir.join("bundle"))
            .await
            .map_err(|e| SandboxError::Backend(e.to_string()))?;
        tokio::fs::create_dir_all(dir.join("state"))
            .await
            .map_err(|e| SandboxError::Backend(e.to_string()))?;
        copy_tree(template, &dir.join("bundle").join("rootfs")).await?;
        self.leases.lock().unwrap().insert(
            lease.id.clone(),
            (lease.clone(), spec.clone(), Default::default()),
        );
        Ok(lease)
    }

    async fn exec(&self, lease: &Lease, req: ExecRequest) -> Result<ExecOutput, SandboxError> {
        self.run(lease, req).await
    }

    async fn put_file(&self, lease: &Lease, path: &str, bytes: Bytes) -> Result<(), SandboxError> {
        let out = self
            .run(
                lease,
                ExecRequest {
                    argv: vec![
                        "/bin/sh".into(),
                        "-c".into(),
                        r#"mkdir -p "$(dirname "$1")" && cat > "$1""#.into(),
                        "sh".into(),
                        path.into(),
                    ],
                    stdin: Some(bytes),
                    timeout: Duration::from_secs(60),
                    ..Default::default()
                },
            )
            .await?;
        if out.exit_code != Some(0) {
            return Err(SandboxError::Backend(
                String::from_utf8_lossy(&out.stderr).into(),
            ));
        }
        Ok(())
    }

    async fn get_file(&self, lease: &Lease, path: &str) -> Result<Bytes, SandboxError> {
        let out = self
            .run(
                lease,
                ExecRequest {
                    argv: vec!["/bin/cat".into(), "--".into(), path.into()],
                    timeout: Duration::from_secs(60),
                    ..Default::default()
                },
            )
            .await?;
        if out.exit_code != Some(0) {
            return Err(SandboxError::Backend(
                String::from_utf8_lossy(&out.stderr).into(),
            ));
        }
        Ok(out.stdout)
    }

    async fn suspend(&self, lease: &Lease) -> Result<SnapshotRef, SandboxError> {
        self.get(lease)?;
        Ok(SnapshotRef {
            id: format!("disk_{}", lease.id),
            lease: lease.id.clone(),
        })
    }

    async fn resume(&self, snap: &SnapshotRef) -> Result<Lease, SandboxError> {
        let m = self.leases.lock().unwrap();
        m.get(&snap.lease)
            .map(|(l, _, _)| l.clone())
            .ok_or(SandboxError::NoLease)
    }

    async fn release(&self, lease: Lease) -> Result<(), SandboxError> {
        self.leases.lock().unwrap().remove(&lease.id);
        let _ = tokio::fs::remove_dir_all(self.lease_dir(&lease.id)).await;
        Ok(())
    }
}
