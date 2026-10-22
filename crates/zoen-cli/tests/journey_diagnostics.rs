//! No service or account: validate the exact safe-log boundary used by World.
#[allow(dead_code)]
#[path = "common/diagnostics.rs"]
mod diagnostics;

use diagnostics::{parse_sync, write_new_record, JourneyDiagnostics};
use std::{
    path::PathBuf,
    process::{ExitStatus, Output},
};

const DEADLINE: &str = r#"{"event":"zoen_sync_wait_deadline","version":1,"timeout_ms":80,"elapsed_ms":80,"state":"online","synced":true,"pending":0,"unknown_ready":true,"uploads_ready":true,"profiles_ready":true,"mls_ready":false}"#;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(types::new_id("safe-sync-diagnostics"));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn records(&self) -> Vec<serde_json::Value> {
        std::fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| {
                let bytes = std::fs::read(entry.unwrap().path()).unwrap();
                assert!(bytes.len() <= 2_048);
                serde_json::from_slice(&bytes).unwrap()
            })
            .collect()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
fn status(success: bool) -> ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    ExitStatus::from_raw(if success { 0 } else { 1 << 8 })
}
#[cfg(windows)]
fn status(success: bool) -> ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    ExitStatus::from_raw(if success { 0 } else { 1 })
}
fn output(stdout: &[u8], stderr: &[u8], success: bool) -> Output {
    Output {
        status: status(success),
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
    }
}

#[test]
fn public_sync_prefix_discards_untrusted_error_suffix_and_never_retains_output() {
    let result = parse_sync(
        b"connection=online synced=false pending=7 error=secret-token-canary\n",
        true,
    );
    let json = serde_json::to_value(result).unwrap();
    assert_eq!(json["state"], "online");
    assert_eq!(json["synced"], false);
    assert_eq!(json["pending"], 7);
    assert_eq!(json["process_success"], true);
    assert!(!json.to_string().contains("canary"));
    assert_eq!(json.as_object().unwrap().len(), 6);
}

#[test]
fn malformed_oversized_overflow_and_extra_line_statuses_become_closed_unparsed() {
    let mut cases = vec![
        b"connection=online synced=false pending=-1\n".to_vec(),
        b"connection=online synced=false pending=18446744073709551616\n".to_vec(),
        b"connection=secret-token-canary synced=true pending=0\n".to_vec(),
        b"connection=online synced=yes pending=0\n".to_vec(),
        b"connection=online synced=true pending=0\nsecret-token-canary\n".to_vec(),
        b"connection=online synced=true pending=0 extra=secret-token-canary\n".to_vec(),
        b"connection=online synced=true\n".to_vec(),
        vec![0xff],
    ];
    cases.push(
        b"connection=online synced=true pending=0 error="
            .iter()
            .copied()
            .chain(std::iter::repeat_n(b'x', 1_025))
            .collect(),
    );
    for text in cases {
        let value = serde_json::to_value(parse_sync(&text, false)).unwrap();
        assert_eq!(value["state"], "unparsed");
        assert!(value["synced"].is_null());
        assert!(value["pending"].is_null());
        assert_eq!(value["process_success"], false);
        assert!(!value.to_string().contains("canary"));
    }
}

#[test]
fn successful_world_capture_writes_nothing_until_the_failure_path() {
    let dir = Directory::new();
    {
        let diagnostics = JourneyDiagnostics::at(Some(dir.0.clone()));
        diagnostics.observe(
            &["sync"],
            &output(b"connection=online synced=false pending=3\n", b"", true),
        );
        assert!(dir.records().is_empty());
        diagnostics.observe(
            &["sync"],
            &output(b"connection=online synced=true pending=0\n", b"", true),
        );
        diagnostics.observe(
            &["page", "secret-argument-canary"],
            &output(b"secret-content-canary", DEADLINE.as_bytes(), false),
        );
        assert!(dir.records().is_empty());
        diagnostics.publish_failure();
    }
    let records = dir.records();
    assert_eq!(records.len(), 4);
    let initial = records.iter().find(|v| v["phase"] == "initial").unwrap();
    let last = records.iter().find(|v| v["phase"] == "last").unwrap();
    assert_eq!(initial["synced"], false);
    assert_eq!(initial["pending"], 3);
    assert_eq!(last["synced"], true);
    assert_eq!(last["pending"], 0);
    let deadline = records
        .iter()
        .find(|v| v["event"] == "zoen_sync_wait_deadline")
        .unwrap();
    assert_eq!(deadline["mls_ready"], false);
    assert!(!serde_json::to_string(&records).unwrap().contains("canary"));
    #[cfg(unix)]
    for entry in std::fs::read_dir(&dir.0).unwrap() {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            entry.unwrap().metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn dropped_successful_capture_leaves_no_artifact() {
    let dir = Directory::new();
    let diagnostics = JourneyDiagnostics::at(Some(dir.0.clone()));
    diagnostics.observe(
        &["sync"],
        &output(
            b"connection=online synced=true pending=0\n",
            DEADLINE.as_bytes(),
            true,
        ),
    );
    drop(diagnostics);
    assert!(dir.records().is_empty());
}

#[test]
fn deadline_records_require_exact_shape_consistent_short_circuit_and_original_time() {
    let dir = Directory::new();
    let diagnostics = JourneyDiagnostics::at(Some(dir.0.clone()));
    let cases = [
        DEADLINE.replace("\"mls_ready\":false", "\"mls_ready\":true"),
        DEADLINE.replace("\"elapsed_ms\":80", "\"elapsed_ms\":79"),
        DEADLINE.replace("\"version\":1", "\"version\":2"),
        DEADLINE.replace("\"unknown_ready\":true", "\"unknown_ready\":false"),
        DEADLINE.replace(",\"mls_ready\":false", ""),
        DEADLINE.replace(
            "\"mls_ready\":false",
            "\"mls_ready\":\"secret-content-canary\"",
        ),
        DEADLINE.replace(
            "\"mls_ready\":false",
            "\"mls_ready\":false,\"args\":\"secret-content-canary\"",
        ),
        DEADLINE.replace("\"version\":1", "\"version\":1,\"version\":1"),
        format!("{}{}", DEADLINE, " ".repeat(2_048)),
    ];
    for line in &cases {
        diagnostics.observe(&[], &output(b"", line.as_bytes(), false));
    }
    diagnostics.publish_failure();
    let records = dir.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["rejected_deadlines"], cases.len());
    assert!(!records[0].to_string().contains("canary"));
}

#[test]
fn diagnostic_record_bounds_are_explicit_and_keep_the_last_observations() {
    let dir = Directory::new();
    let diagnostics = JourneyDiagnostics::at(Some(dir.0.clone()));
    for _ in 0..20 {
        diagnostics.observe(&[], &output(b"", DEADLINE.as_bytes(), false));
    }
    diagnostics.publish_failure();
    let records = dir.records();
    assert_eq!(records.len(), 17);
    let summary = records
        .iter()
        .find(|v| v["event"] == "zoen_journey_sync_diagnostics")
        .unwrap();
    assert_eq!(summary["discarded_deadlines"], 4);
}

#[test]
fn existing_output_and_symlink_targets_are_never_overwritten() {
    let dir = Directory::new();
    let file = dir.0.join("existing.json");
    std::fs::write(&file, b"existing-private-canary").unwrap();
    assert!(write_new_record(&file, b"{}").is_err());
    assert_eq!(std::fs::read(&file).unwrap(), b"existing-private-canary");
    assert!(write_new_record(&dir.0.join("too-large.json"), &vec![b'x'; 2_048]).is_err());
    #[cfg(unix)]
    {
        let link = dir.0.join("symlink.json");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        assert!(write_new_record(&link, b"{}").is_err());
        assert_eq!(std::fs::read(&file).unwrap(), b"existing-private-canary");
    }
}

#[test]
fn disabled_missing_and_symlink_directories_cannot_mask_the_original_failure() {
    let dir = Directory::new();
    for target in [None, Some(dir.0.join("missing"))] {
        let diagnostics = JourneyDiagnostics::at(target);
        diagnostics.observe(
            &[],
            &output(b"secret-content-canary", DEADLINE.as_bytes(), false),
        );
        diagnostics.publish_failure();
    }
    assert!(dir.records().is_empty());
    #[cfg(unix)]
    {
        let link = dir.0.join("directory-link");
        let target = Directory::new();
        std::os::unix::fs::symlink(&target.0, &link).unwrap();
        let diagnostics = JourneyDiagnostics::at(Some(link));
        diagnostics.observe(&[], &output(b"", DEADLINE.as_bytes(), false));
        diagnostics.publish_failure();
        assert!(target.records().is_empty());
    }
}
