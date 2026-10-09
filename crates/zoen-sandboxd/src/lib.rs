//! # zoen-sandboxd
//!
//! The sandbox node agent (ADR 0028 §2): runs Firecracker microVMs for the microVM tier.
//! One process per host; no pod per sandbox. See [`provider`] for the lifecycle, [`jail`] for
//! how each VMM is confined, and `zoen-guestd` for the agent inside the VM.

pub mod api;
pub mod jail;
pub mod provider;
pub mod vsock;

pub use jail::Limits;
pub use provider::{AcquireTiming, FirecrackerProvider, Shape};

use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct FirecrackerConfig {
    pub firecracker: PathBuf,
    pub jailer: PathBuf,
    pub kernel: PathBuf,
    /// The guest root filesystem (ext4, read-only, `zoen-guestd` at `/sbin/zoen-guestd`).
    pub rootfs: PathBuf,
    /// Templates, suspended snapshots and chroots. Must be on one filesystem with the files
    /// above for hard links (falls back to copies).
    pub work: PathBuf,
    /// The unprivileged user each VMM runs as.
    pub uid: u32,
    pub gid: u32,
    /// Run the jailer through `sudo -n` (development and CI; production runs as root).
    pub sudo: bool,
    /// A cgroup v2 directory under `/sys/fs/cgroup` with cpu, memory and pids enabled for its
    /// children and writable by `uid` (see `scripts/firecracker.sh cgroup`).
    pub cgroup_parent: String,
    /// CPU quota per vCPU, in percent of one core.
    pub cpu_quota_pct: u32,
    /// Memory the VMM itself may use on top of guest RAM.
    pub vmm_overhead_mib: u32,
    /// Paused VMs kept per shape.
    pub pool_size: usize,
    pub boot_timeout: Duration,
}

impl FirecrackerConfig {
    /// From the exports of `scripts/firecracker.sh env`; `None` when they are missing or
    /// the host has no `/dev/kvm`.
    pub fn from_env(work: PathBuf) -> Option<Self> {
        let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
        // The jailer gives each VMM its own /dev/kvm node owned by `uid`, so the caller
        // doesn't need to be in the kvm group; the device only has to exist.
        if !std::path::Path::new("/dev/kvm").exists() {
            return None;
        }
        // SAFETY: getuid/getgid never fail.
        let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
        Some(FirecrackerConfig {
            firecracker: var("ZOEN_FIRECRACKER")?,
            jailer: var("ZOEN_JAILER")?,
            kernel: var("ZOEN_VMLINUX")?,
            rootfs: var("ZOEN_GUEST_ROOTFS")?,
            work,
            uid,
            gid,
            sudo: uid != 0,
            cgroup_parent: std::env::var("ZOEN_SANDBOX_CGROUP")
                .unwrap_or_else(|_| "zoen-sandbox".into()),
            cpu_quota_pct: 100,
            vmm_overhead_mib: 64,
            pool_size: 2,
            boot_timeout: Duration::from_secs(20),
        })
    }
}
