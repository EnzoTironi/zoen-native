#![cfg(target_os = "linux")]

use relay_admission::managed_images::{AnchorError, AnchorWriter, Deployment, Recovery};
use ring::rand::{SecureRandom, SystemRandom};
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};

type TestResult = Result<(), Box<dyn Error>>;
const TEST: &str = "forked_descriptors_do_not_extend_anchor_authority";

struct Home(PathBuf);
impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn forked_descriptors_do_not_extend_anchor_authority() -> TestResult {
    if let Ok(mode) = std::env::var("ZOEN_ANCHOR_FORK_HELPER") {
        return helper(&mode);
    }
    let mut nonce = [0; 16];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| "test randomness")?;
    let home = Home(std::env::temp_dir().join(format!(
        "zoen-anchor-fork-{}-{}",
        std::process::id(),
        event_log::sha256_hex(&nonce)
    )));
    fs::create_dir(&home.0)?;
    for mode in ["retirement", "generation", "writer"] {
        let output = Command::new("python3")
            .arg("-c")
            .arg(include_str!("support/anchor_fork_trace.py"))
            .arg(home.0.join(mode))
            .arg(std::env::current_exe()?)
            .args([TEST, "--exact", "--nocapture"])
            .env("ZOEN_ANCHOR_FORK_HELPER", mode)
            .output()?;
        assert!(
            output.status.success(),
            "{mode}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn helper(mode: &str) -> TestResult {
    let run = std::env::var("PROOF_RUN")?;
    let run = Path::new(&run);
    let deployment = Deployment {
        namespace: "fork-fixture".into(),
        witness: "a".repeat(64),
        source: "relay".into(),
        agent: "agent".into(),
        device: "device".into(),
    };
    let mut writer = Some(AnchorWriter::create(
        &run.join("anchor"),
        deployment.clone(),
    )?);
    let initial = writer
        .as_ref()
        .ok_or("missing writer")?
        .reader()
        .checkpoint()?;
    let mut generation = None;
    let mut retirement = None;
    if mode == "retirement" {
        retirement = Some(writer.as_mut().ok_or("missing writer")?.prepare(
            &"b".repeat(64),
            &initial,
            &initial,
            true,
        )?);
    } else if mode == "generation" {
        generation = Some(
            writer
                .as_ref()
                .ok_or("missing writer")?
                .reader()
                .generation_guard(&initial)?,
        );
    }
    let spawn = thread::spawn(|| Command::new("/bin/true").status());
    let start = Instant::now();
    while !run.join("pre-exec-held").exists() {
        if start.elapsed() > Duration::from_secs(20) {
            return Err("missing pre-exec stop".into());
        }
        thread::sleep(Duration::from_millis(2));
    }
    match mode {
        "retirement" => {
            let next = retirement.ok_or("missing retirement")?.next;
            let owner = writer.as_mut().ok_or("missing writer")?;
            assert_eq!(owner.reconcile(&next, &next)?, Recovery::BothAfter);
            assert_eq!(
                owner.reader().generation_guard(&initial).err(),
                Some(AnchorError::Unreconciled)
            );
            let _current = owner.reader().generation_guard(&next)?;
        }
        "generation" => {
            drop(generation.take());
            let owner = writer.as_mut().ok_or("missing writer")?;
            owner.prepare(&"b".repeat(64), &initial, &initial, true)?;
            assert_eq!(owner.reconcile(&initial, &initial)?, Recovery::BothBefore);
        }
        "writer" => {
            drop(writer.take());
            let _next_writer = AnchorWriter::open(&run.join("anchor"), deployment)?;
        }
        _ => return Err("unknown fork fixture mode".into()),
    }
    fs::write(
        run.join("observed.json"),
        b"{\"authority_released_before_child_exec\":true}",
    )?;
    assert!(spawn.join().map_err(|_| "spawn panicked")??.success());
    Ok(())
}
