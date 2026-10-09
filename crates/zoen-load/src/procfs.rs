//! CPU time and resident memory of the processes under test (Linux `/proc`).

use std::sync::OnceLock;

fn ticks_per_second() -> f64 {
    static TCK: OnceLock<f64> = OnceLock::new();
    *TCK.get_or_init(|| {
        std::process::Command::new("getconf")
            .arg("CLK_TCK")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(100.0)
    })
}

/// User plus system CPU seconds the process has used so far.
pub fn cpu_seconds(pid: u32) -> Option<f64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 2..];
    let f: Vec<&str> = rest.split_whitespace().collect();
    let utime: f64 = f.get(11)?.parse().ok()?;
    let stime: f64 = f.get(12)?.parse().ok()?;
    Some((utime + stime) / ticks_per_second())
}

/// Resident set size in bytes.
pub fn rss_bytes(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

/// CPU seconds of each named process, sampled together.
pub fn snapshot(pids: &[(String, u32)]) -> Vec<f64> {
    pids.iter()
        .map(|(_, p)| cpu_seconds(*p).unwrap_or(0.0))
        .collect()
}
