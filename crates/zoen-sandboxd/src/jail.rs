//! One Firecracker process under the jailer: its own chroot, uid, cgroup (memory, CPU and
//! process limits) and network namespace-free setup. The VM dies with [`Vm::kill`], which
//! also removes its chroot and cgroup.

use crate::FirecrackerConfig;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::process::Command;

/// Resource limits for one VM, enforced by the host kernel (cgroup v2), not by the guest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// `memory.max`: guest RAM plus the VMM's own overhead.
    pub memory_bytes: u64,
    /// `cpu.max`: quota per 100 ms period, in microseconds.
    pub cpu_quota_us: u64,
    /// `pids.max`: Firecracker's threads (one per vCPU, plus API and VMM).
    pub pids: u32,
}

impl Limits {
    pub fn for_shape(cfg: &FirecrackerConfig, vcpu: u8, mem_mib: u32) -> Self {
        Limits {
            memory_bytes: (u64::from(mem_mib) + u64::from(cfg.vmm_overhead_mib)) << 20,
            cpu_quota_us: u64::from(vcpu) * 1000 * u64::from(cfg.cpu_quota_pct),
            pids: 32,
        }
    }
}

#[derive(Debug)]
pub struct Vm {
    pub id: String,
    /// `<work>/jail/firecracker/<id>`; the chroot is its `root/`.
    pub dir: PathBuf,
    pub root: PathBuf,
    pub cgroup: PathBuf,
    pub pid: i32,
    pub limits: Limits,
    /// The chroot directory, held open: sockets are reached through `/proc/self/fd/<n>/`,
    /// because full chroot paths can exceed the 108-byte limit on Unix socket paths.
    root_dir: Option<std::fs::File>,
}

fn new_id() -> String {
    let mut b = [0u8; 8];
    getrandom::getrandom(&mut b).expect("os randomness");
    format!("vm-{}", hex::encode(b))
}

/// Hard-links (or, across filesystems, copies) a file into the chroot.
async fn place(src: &Path, dst: &Path) -> std::io::Result<()> {
    match tokio::fs::hard_link(src, dst).await {
        Ok(()) => Ok(()),
        Err(_) => tokio::fs::copy(src, dst).await.map(|_| ()),
    }
}

impl Vm {
    /// A short path to `name` inside the chroot, usable for Unix sockets.
    pub fn sock_path(&self, name: &str) -> PathBuf {
        use std::os::fd::AsRawFd;
        match &self.root_dir {
            Some(f) => PathBuf::from(format!("/proc/self/fd/{}/{name}", f.as_raw_fd())),
            None => self.root.join(name),
        }
    }

    pub fn api(&self) -> PathBuf {
        self.sock_path("api.sock")
    }

    pub fn vsock(&self) -> PathBuf {
        self.sock_path("v.sock")
    }

    /// Starts the jailer (through `sudo -n` when configured) with `files` placed in the chroot
    /// under the given names. Returns once the API socket is up.
    pub async fn launch(
        cfg: &FirecrackerConfig,
        limits: Limits,
        files: &[(&Path, &str)],
    ) -> std::io::Result<Vm> {
        let id = new_id();
        let base = cfg.work.join("jail");
        let dir = base.join("firecracker").join(&id);
        let root = dir.join("root");
        tokio::fs::create_dir_all(&root).await?;
        for (src, name) in files {
            place(src, &root.join(name)).await?;
        }
        tokio::fs::write(root.join("fc.log"), b"").await?;
        let cgroup = PathBuf::from("/sys/fs/cgroup")
            .join(&cfg.cgroup_parent)
            .join(&id);
        let mut cmd = if cfg.sudo {
            let mut c = Command::new("sudo");
            c.arg("-n").arg(&cfg.jailer);
            c
        } else {
            Command::new(&cfg.jailer)
        };
        cmd.arg("--id")
            .arg(&id)
            .arg("--exec-file")
            .arg(&cfg.firecracker)
            .arg("--uid")
            .arg(cfg.uid.to_string())
            .arg("--gid")
            .arg(cfg.gid.to_string())
            .arg("--chroot-base-dir")
            .arg(&base)
            .args(["--cgroup-version", "2", "--parent-cgroup"])
            .arg(&cfg.cgroup_parent)
            .arg("--cgroup")
            .arg(format!("memory.max={}", limits.memory_bytes))
            .arg("--cgroup")
            .arg(format!("cpu.max={} 100000", limits.cpu_quota_us))
            .arg("--cgroup")
            .arg(format!("pids.max={}", limits.pids))
            .arg("--daemonize")
            .args([
                "--",
                "--api-sock",
                "/api.sock",
                "--log-path",
                "/fc.log",
                "--level",
                "Warning",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        let out = tokio::time::timeout(Duration::from_secs(10), cmd.output())
            .await
            .map_err(|_| std::io::Error::other("jailer did not return"))??;
        if !out.status.success() {
            let _ = tokio::fs::remove_dir_all(&dir).await;
            return Err(std::io::Error::other(format!(
                "jailer: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let pid = loop {
            if let Ok(s) = tokio::fs::read_to_string(root.join("firecracker.pid")).await {
                if let Ok(p) = s.trim().parse::<i32>() {
                    break p;
                }
            }
            if Instant::now() > deadline {
                return Err(std::io::Error::other("no firecracker.pid"));
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        };
        let root_dir = std::fs::File::open(&root).ok();
        let vm = Vm {
            id,
            dir,
            root,
            cgroup,
            pid,
            limits,
            root_dir,
        };
        while !vm.root.join("api.sock").exists() {
            if Instant::now() > deadline {
                vm.kill().await;
                return Err(std::io::Error::other("no API socket"));
            }
            tokio::time::sleep(Duration::from_micros(250)).await;
        }
        Ok(vm)
    }

    /// Kills the VMM, then removes its cgroup and chroot. Idempotent; never blocks long.
    pub async fn kill(&self) {
        self.kill_now();
        // The cgroup can be removed once the VMM has fully exited.
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.cgroup.exists() {
            if tokio::fs::remove_dir(&self.cgroup).await.is_ok() || Instant::now() > deadline {
                break;
            }
            self.kill_now();
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let _ = tokio::fs::remove_dir_all(&self.dir).await;
    }

    /// The synchronous part of [`kill`](Self::kill), for `Drop`.
    pub fn kill_now(&self) {
        // Only signal the pid while it is still this VM's VMM (it lives in our cgroup).
        let ours = std::fs::read_to_string(self.cgroup.join("cgroup.procs"))
            .map(|s| s.lines().any(|l| l.trim() == self.pid.to_string()))
            .unwrap_or(false);
        if ours {
            // SAFETY: plain kill(2).
            unsafe { libc::kill(self.pid, libc::SIGKILL) };
        }
    }

    pub fn kill_blocking(&self) {
        self.kill_now();
        for _ in 0..1000 {
            if !self.cgroup.exists() || std::fs::remove_dir(&self.cgroup).is_ok() {
                break;
            }
            self.kill_now();
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Kills VMs left behind by a process that died without cleaning up (their chroots are still
/// under `<work>/jail`). Run at start-up.
pub fn sweep(cfg: &FirecrackerConfig) -> usize {
    let base = cfg.work.join("jail").join("firecracker");
    let Ok(entries) = std::fs::read_dir(&base) else {
        return 0;
    };
    let mut n = 0;
    for e in entries.flatten() {
        let id = e.file_name().to_string_lossy().to_string();
        let pid = std::fs::read_to_string(e.path().join("root/firecracker.pid"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let vm = Vm {
            cgroup: PathBuf::from("/sys/fs/cgroup")
                .join(&cfg.cgroup_parent)
                .join(&id),
            id,
            root: e.path().join("root"),
            dir: e.path(),
            pid,
            limits: Limits {
                memory_bytes: 0,
                cpu_quota_us: 0,
                pids: 0,
            },
            root_dir: None,
        };
        vm.kill_blocking();
        n += 1;
    }
    n
}
