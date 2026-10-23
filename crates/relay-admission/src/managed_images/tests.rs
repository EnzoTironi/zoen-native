use super::*;
use ring::rand::{SecureRandom, SystemRandom};
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Home(PathBuf);
impl Home {
    fn new() -> Result<Self, std::io::Error> {
        let mut bytes = [0; 16];
        SystemRandom::new()
            .fill(&mut bytes)
            .map_err(|_| std::io::Error::other("test randomness"))?;
        let name = event_log::sha256_hex(&bytes);
        Ok(Self(std::env::temp_dir().join(format!(
            "zoen-image-anchor-{}-{name}",
            std::process::id()
        ))))
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn deployment() -> Deployment {
    Deployment {
        namespace: "fixture".into(),
        witness: "a".repeat(64),
        source: "relay-cell".into(),
        agent: "agent".into(),
        device: "device".into(),
    }
}
fn effect() -> String {
    event_log::sha256_hex(b"original immutable dispatch binding and claim nonce")
}

#[test]
fn coordinated_old_snapshot_is_detected_even_when_both_stores_agree() -> TestResult {
    let home = Home::new()?;
    let mut writer = AnchorWriter::create(&home.0, deployment())?;
    let before = writer.reader().checkpoint()?;
    let pending = writer.prepare(&effect(), &before, &before, false)?;
    assert_eq!(
        writer.reconcile(&pending.next, &pending.next)?,
        Recovery::BothAfter
    );
    drop(writer);
    let mut restarted = AnchorWriter::open(&home.0, deployment())?;
    assert_eq!(
        restarted.reconcile(&before, &before),
        Err(AnchorError::Unreconciled)
    );
    assert_eq!(restarted.reader().checkpoint()?, pending.next);
    Ok(())
}

#[test]
fn crash_before_both_store_commits_aborts_only_the_pending_checkpoint() -> TestResult {
    let home = Home::new()?;
    let mut writer = AnchorWriter::create(&home.0, deployment())?;
    let before = writer.reader().checkpoint()?;
    writer.prepare(&effect(), &before, &before, false)?;
    drop(writer);
    let mut restarted = AnchorWriter::open(&home.0, deployment())?;
    assert_eq!(
        restarted.reader().checkpoint(),
        Err(AnchorError::Unreconciled)
    );
    assert_eq!(restarted.reconcile(&before, &before)?, Recovery::BothBefore);
    assert_eq!(restarted.pending()?, None);
    assert_eq!(restarted.reader().checkpoint()?, before);
    Ok(())
}

#[test]
fn crash_after_both_store_commits_recovers_observation_without_send_permit() -> TestResult {
    let home = Home::new()?;
    let mut writer = AnchorWriter::create(&home.0, deployment())?;
    let before = writer.reader().checkpoint()?;
    let pending = writer.prepare(&effect(), &before, &before, false)?;
    drop(writer);
    let mut restarted = AnchorWriter::open(&home.0, deployment())?;
    assert_eq!(
        restarted.reconcile(&pending.next, &pending.next)?,
        Recovery::BothAfter
    );
    assert_eq!(
        restarted.reconcile(&pending.next, &pending.next)?,
        Recovery::Clean
    );
    assert_eq!(restarted.reader().checkpoint()?, pending.next);
    Ok(())
}

#[test]
fn partial_checkpoint_is_retained_until_original_store_evidence_is_restored() -> TestResult {
    for sql_first in [false, true] {
        let home = Home::new()?;
        let mut writer = AnchorWriter::create(&home.0, deployment())?;
        let before = writer.reader().checkpoint()?;
        let pending = writer.prepare(&effect(), &before, &before, false)?;
        let (sql, fdb, expected) = if sql_first {
            (&pending.next, &before, Recovery::FdbBefore)
        } else {
            (&before, &pending.next, Recovery::SqlBefore)
        };
        assert_eq!(writer.reconcile(sql, fdb)?, expected);
        assert_eq!(writer.pending()?, Some(pending.clone()));
        assert!(matches!(
            writer.reader().generation_guard(&pending.next),
            Err(AnchorError::Unreconciled)
        ));
        assert_eq!(
            writer.prepare(&effect(), sql, fdb, false),
            Err(AnchorError::Unreconciled)
        );
        // Only an exact repaired observation resolves this record; no database
        // mutation, witness creation or provider permit is exposed by the anchor.
        assert_eq!(
            writer.reconcile(&pending.next, &pending.next)?,
            Recovery::BothAfter
        );
    }
    Ok(())
}

#[test]
fn corruption_missing_state_symlink_and_wrong_deployment_are_not_bootstrapped() -> TestResult {
    let home = Home::new()?;
    let writer = AnchorWriter::create(&home.0, deployment())?;
    assert!(!format!("{:?}", deployment()).contains(&deployment().witness));
    drop(writer);
    let mut other = deployment();
    other.device = "other".into();
    assert!(matches!(
        AnchorWriter::open(&home.0, other),
        Err(AnchorError::Invalid)
    ));
    std::fs::write(home.0.join("checkpoint.json"), b"{}")?;
    assert!(matches!(
        AnchorWriter::open(&home.0, deployment()),
        Err(AnchorError::Invalid)
    ));
    std::fs::remove_file(home.0.join("checkpoint.json"))?;
    assert!(AnchorWriter::open(&home.0, deployment()).is_err());
    assert!(AnchorWriter::create(&home.0, deployment()).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(home.0.join("writer.lock"), home.0.join("checkpoint.json"))?;
        assert!(matches!(
            AnchorWriter::open(&home.0, deployment()),
            Err(AnchorError::Invalid)
        ));
    }
    Ok(())
}

#[test]
fn generation_retirement_drains_effects_and_rejects_old_checkpoint() -> TestResult {
    let home = Home::new()?;
    let mut writer = AnchorWriter::create(&home.0, deployment())?;
    let before = writer.reader().checkpoint()?;
    let effect_guard = writer.reader().generation_guard(&before)?;
    assert_eq!(
        writer.prepare(&effect(), &before, &before, true),
        Err(AnchorError::Busy)
    );
    drop(effect_guard);
    let retirement = writer.prepare(&effect(), &before, &before, true)?;
    assert_eq!(retirement.next.generation, before.generation + 1);
    assert!(matches!(
        writer.reader().generation_guard(&before),
        Err(AnchorError::Busy)
    ));
    assert_eq!(
        writer.reconcile(&retirement.next, &retirement.next)?,
        Recovery::BothAfter
    );
    assert!(matches!(
        writer.reader().generation_guard(&before),
        Err(AnchorError::Unreconciled)
    ));
    let _new_effect = writer.reader().generation_guard(&retirement.next)?;
    Ok(())
}

fn child(home: &Path, mode: &str) -> std::io::Result<std::process::Command> {
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .arg("managed_images::tests::managed_anchor_process_helper")
        .arg("--exact")
        .arg("--nocapture")
        .env("ZOEN_ANCHOR_TEST_HOME", home)
        .env("ZOEN_ANCHOR_TEST_MODE", mode);
    Ok(command)
}

#[test]
fn managed_anchor_process_helper() -> TestResult {
    let Some(home) = std::env::var_os("ZOEN_ANCHOR_TEST_HOME") else {
        return Ok(());
    };
    let mode = std::env::var("ZOEN_ANCHOR_TEST_MODE")?;
    if mode == "writer" {
        match AnchorWriter::open(Path::new(&home), deployment()) {
            Ok(_writer) => {}
            Err(AnchorError::Busy) => std::process::exit(73),
            Err(error) => return Err(error.into()),
        }
    } else {
        use std::io::Read;
        let reader = AnchorReader::open(Path::new(&home), deployment())?;
        let checkpoint = reader.checkpoint()?;
        let _effect = reader.generation_guard(&checkpoint)?;
        std::fs::write(Path::new(&home).join("effect-ready"), b"held")?;
        let mut byte = [0];
        assert_eq!(std::io::stdin().read(&mut byte)?, 0);
    }
    Ok(())
}

#[test]
fn actual_process_writer_and_effect_locks_prevent_early_takeover() -> TestResult {
    use std::process::Stdio;
    let home = Home::new()?;
    let mut writer = AnchorWriter::create(&home.0, deployment())?;
    let before = writer.reader().checkpoint()?;
    assert_eq!(child(&home.0, "writer")?.status()?.code(), Some(73));
    let mut holder = child(&home.0, "effect")?.stdin(Stdio::piped()).spawn()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !home.0.join("effect-ready").exists() {
        assert!(
            holder.try_wait()?.is_none(),
            "effect child exited before holding lock"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "effect child did not acquire"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        writer.prepare(&effect(), &before, &before, true),
        Err(AnchorError::Busy)
    );
    drop(holder.stdin.take());
    assert!(holder.wait()?.success());
    writer.prepare(&effect(), &before, &before, true)?;
    drop(writer);
    assert!(child(&home.0, "writer")?.status()?.success());
    Ok(())
}
