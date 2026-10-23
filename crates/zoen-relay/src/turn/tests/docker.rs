//! Explicit opt-in: a disposable upstream TURN server, never the local relay stack.
use super::*;
use ring::rand::{SecureRandom, SystemRandom};
use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf};

struct InputFile(PathBuf);
impl Drop for InputFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
#[ignore = "opt-in Docker/TLS proof; requires ZOEN_TURN_DOCKER_PROOF=1 and an empty evidence directory"]
fn production_credentials_relay_over_real_tls_coturn_and_invalid_auth_is_refused() {
    assert_eq!(std::env::var("ZOEN_TURN_DOCKER_PROOF").as_deref(), Ok("1"));
    let evidence = PathBuf::from(std::env::var("ZOEN_TURN_PROOF_DIR").expect("proof directory"));
    assert!(evidence.is_absolute(), "proof directory must be absolute");
    std::fs::create_dir_all(&evidence).unwrap();
    assert_eq!(std::fs::read_dir(&evidence).unwrap().count(), 0);
    let python = std::env::var("ZOEN_TURN_PROOF_PYTHON").expect("explicit Python executable");
    let mut random = [0; 32];
    SystemRandom::new().fill(&mut random).unwrap();
    let secret = hex::encode(random);
    let issuer = Service::configured(
        Some("turns:turn.zoen-proof.invalid:5349?transport=tcp".into()),
        Some(secret.clone()),
    )
    .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let valid = issuer
        .issue("synthetic-caller", "synthetic-device", now)
        .unwrap();
    let expired = issuer
        .issue("synthetic-caller", "synthetic-device", now - 180_000)
        .unwrap();
    assert!(valid.valid() && expired.valid() && expired.expires_at_ms < now);
    let private = InputFile(evidence.join(".issuer-input.json"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&private.0)
        .unwrap();
    // Values are consumed from a private file, never process arguments or logs.
    // Both positive and expired credentials use the production issuer above.
    let input = serde_json::json!({
        "secret": secret,
        "valid": {"username": valid.username, "credential": valid.credential},
        "expired": {"username": expired.username, "credential": expired.credential}
    });
    file.write_all(serde_json::to_string(&input).unwrap().as_bytes())
        .unwrap();
    drop(file);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = std::process::Command::new(python)
        .arg(root.join("scripts/call-turn-proof.py"))
        .arg("--input")
        .arg(&private.0)
        .arg("--evidence")
        .arg(&evidence)
        .status()
        .expect("run isolated TURN proof");
    assert!(
        result.success(),
        "TURN proof failed; inspect redacted report.json"
    );
    assert!(!private.0.exists(), "issuer input must be retired");
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(evidence.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["status"], "PASS");
    assert_eq!(report["cleanup"]["complete"], true);
}
