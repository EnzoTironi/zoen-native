use super::*;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| (*s).into()).collect()
}
fn sample() -> serde_json::Value {
    serde_json::json!({"version":1,"kind":"user","persona":"Edit reviewed presentations.","rules":["Preserve the source."],"state":{},"model_tier":"cheap"})
}
fn encoded() -> Vec<u8> {
    definition(&serde_json::to_vec(&sample()).unwrap()).unwrap()
}

#[test]
fn only_enrollment_and_host_share_the_lifetime_lock() {
    for name in [REQUEST, WAIT, CANCEL, "agent-host"] {
        assert!(requires_home_lock(name));
    }
    for name in ["agent-init", "agent-keys", "status", "agent-enroll"] {
        assert!(!requires_home_lock(name));
    }
}

#[test]
fn request_requires_trusted_owner_and_relay_without_implicit_defaults() {
    let flags = args(&[
        "--owner",
        "public-owner",
        "--relay",
        "http://127.0.0.1:8880",
        "--name",
        "Host",
        "--handle",
        "host",
        "--definition",
        "definition.json",
    ]);
    assert!(matches!(
        Action::parse(REQUEST, &flags, 8000, false).unwrap(),
        Some(Action::Request { .. })
    ));
    for key in ["--owner", "--relay", "--definition", "--name", "--handle"] {
        let mut missing = flags.clone();
        let pos = missing.iter().position(|s| s == key).unwrap();
        missing.drain(pos..pos + 2);
        assert!(Action::parse(REQUEST, &missing, 8000, false).is_err());
    }
}

#[test]
fn ambiguous_or_unknown_operator_arguments_are_rejected() {
    for flags in [
        args(&["--offer-digest"]),
        args(&["--offer-digest", "--owner"]),
        args(&["--unknown", "value"]),
        args(&[
            "--offer-digest",
            &"a".repeat(64),
            "--offer-digest",
            &"b".repeat(64),
        ]),
    ] {
        assert!(Action::parse(CANCEL, &flags, 8000, false).is_err());
    }
    assert_eq!(Action::parse("whoami", &[], 8000, false).unwrap(), None);
}

#[test]
fn wait_has_explicit_finite_timeout_and_never_runs_offline() {
    assert_eq!(
        Action::parse(WAIT, &[], 8000, false).unwrap(),
        Some(Action::Wait { timeout_ms: 8000 })
    );
    for timeout in [0, MAX_WAIT_MS + 1, u64::MAX] {
        assert!(Action::parse(WAIT, &[], timeout, false).is_err());
    }
    assert!(Action::parse(WAIT, &[], 8000, true).is_err());
    assert!(Action::parse(WAIT, &args(&["--reset", "yes"]), 8000, false).is_err());
}

#[test]
fn cancellation_needs_the_exact_public_digest() {
    assert_eq!(
        Action::parse(
            CANCEL,
            &args(&["--offer-digest", &"a".repeat(64)]),
            8000,
            true
        )
        .unwrap(),
        Some(Action::Cancel {
            digest: "a".repeat(64)
        })
    );
    for digest in [
        "A".repeat(64),
        "a".repeat(63),
        "g".repeat(64),
        String::new(),
    ] {
        assert!(Action::parse(CANCEL, &args(&["--offer-digest", &digest]), 8000, false).is_err());
    }
}

#[test]
fn real_definition_encoding_preserves_every_reviewed_field() {
    let value = sample();
    let canonical = encoded();
    let parsed: Definition = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(canonical, parsed.encode().unwrap());
    assert_eq!(Definition::decode(&canonical).unwrap(), parsed);
    assert_eq!(
        definition(&serde_json::to_vec_pretty(&value).unwrap()).unwrap(),
        canonical
    );
}

#[test]
fn unsupported_definition_is_refused_instead_of_clamped_or_defaulted() {
    for (field, bad) in [
        ("version", serde_json::json!(2)),
        ("kind", serde_json::json!("creator")),
        ("model_tier", serde_json::json!("premium")),
        ("state", serde_json::json!({"write":true})),
        ("persona", serde_json::json!("")),
        ("rules", serde_json::json!(["bad\nrule"])),
    ] {
        let mut value = sample();
        value[field] = bad;
        assert!(definition(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    for field in ["state", "model_tier"] {
        let mut value = sample();
        value.as_object_mut().unwrap().remove(field);
        assert!(definition(&serde_json::to_vec(&value).unwrap()).is_err());
    }
    let mut value = sample();
    value["unreviewed"] = serde_json::json!(true);
    assert!(definition(&serde_json::to_vec(&value).unwrap()).is_err());
    assert!(definition(br#"{"version":1,"version":1,"kind":"user","persona":"x","rules":[],"state":{},"model_tier":"cheap"}"#).is_err());
}

#[test]
fn relay_reconciliation_accepts_only_absent_or_the_exact_frozen_definition() {
    let expected = encoded();
    assert_eq!(
        definition_state("active", 0, "", &[], &expected).unwrap(),
        DefinitionState::Absent
    );
    assert_eq!(
        definition_state("paused", 1, "User", &expected, &expected).unwrap(),
        DefinitionState::Confirmed
    );
    for (status, version, kind, bytes) in [
        ("revoked", 1, "User", expected.as_slice()),
        ("unknown", 1, "User", expected.as_slice()),
        ("active", 2, "User", expected.as_slice()),
        ("active", 1, "Creator", expected.as_slice()),
        ("active", 0, "User", &[]),
        ("active", 1, "User", b"different"),
    ] {
        assert!(definition_state(status, version, kind, bytes, &expected).is_err());
    }
}

struct Home(PathBuf);
impl Home {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "zoen-enroll-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn vault(&self) -> FileVault {
        FileVault {
            dir: self.0.join("vault"),
        }
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn empty_home_preflight_creates_no_keys_or_account() {
    let home = Home::new();
    preflight_slots(&home.vault()).unwrap();
    assert!(!home.vault().dir.exists());
}

#[test]
fn recovery_and_cancel_share_storage_health_checks_without_requiring_a_terminal() {
    let home = Home::new();
    let vault = home.vault();
    let actions = [
        Action::Wait { timeout_ms: 8000 },
        Action::Cancel {
            digest: "a".repeat(64),
        },
    ];
    for action in &actions {
        action.preflight(&vault).unwrap();
    }
    assert!(!vault.dir.exists());
    std::fs::create_dir(&vault.dir).unwrap();
    let root = vault.dir.join("agent_root");
    std::fs::create_dir(&root).unwrap();
    for action in &actions {
        assert!(action.preflight(&vault).is_err());
        assert!(root.is_dir());
    }
    std::fs::remove_dir(&root).unwrap();
    std::fs::write(&root, b"retained-malformed-root").unwrap();
    for action in &actions {
        assert!(action.preflight(&vault).is_err());
        assert_eq!(std::fs::read(&root).unwrap(), b"retained-malformed-root");
    }
}

#[test]
fn unreadable_or_malformed_existing_root_is_never_treated_as_missing() {
    let home = Home::new();
    let vault = home.vault();
    std::fs::create_dir(&vault.dir).unwrap();
    let path = vault.dir.join("agent_root");
    std::fs::create_dir(&path).unwrap();
    assert!(preflight_slots(&vault).is_err());
    assert!(path.is_dir());
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, b"short-root").unwrap();
    assert!(preflight_slots(&vault).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"short-root");
    #[cfg(unix)]
    {
        std::fs::remove_file(&path).unwrap();
        let outside = home.0.join("outside");
        std::fs::write(&outside, [5; 32]).unwrap();
        std::os::unix::fs::symlink(&outside, &path).unwrap();
        assert!(preflight_slots(&vault).is_err());
        assert_eq!(std::fs::read(outside).unwrap(), [5; 32]);
    }
}

#[test]
fn legacy_root_and_complete_core_seed_shapes_are_preserved() {
    let home = Home::new();
    let vault = home.vault();
    for key in [
        "agent_root",
        "zoen.identity.v1",
        "zoen.device.v1",
        "zoen.agreement.v1",
    ] {
        vault.write_bytes(key, &[7; 32]).unwrap();
    }
    preflight_slots(&vault).unwrap();
    assert_eq!(vault.read_bytes("agent_root").unwrap(), [7; 32]);
    vault.write_bytes("zoen.provider-setup.v1", b"{}").unwrap();
    assert!(preflight_slots(&vault).is_err());
    assert_eq!(vault.read_bytes("zoen.provider-setup.v1").unwrap(), b"{}");
}
