//! Synthetic publishers only. Exercises the actual publication IO and normal verifier.
use super::*;
use std::{fs, os::unix::fs::PermissionsExt};

struct Fixture {
    directory: tempfile::TempDir,
    component: std::path::PathBuf,
    manifest: std::path::PathBuf,
    key: std::path::PathBuf,
    output: std::path::PathBuf,
    publisher: String,
}
impl Fixture {
    fn new() -> Result<Self> {
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let component = directory.path().join("component.wasm");
        // Genuine empty component-model container; no engine validation is claimed here.
        fs::write(&component, b"\0asm\x0d\0\x01\0").map_err(|e| e.to_string())?;
        let key = directory.path().join("synthetic-seed");
        fs::write(&key, [31_u8; 32]).map_err(|e| e.to_string())?;
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
        let manifest = directory.path().join("unsigned.json");
        let output = directory.path().join("signed.json");
        let publisher = hex::encode(SigningKey::from_bytes(&[31; 32]).verifying_key().to_bytes());
        let fixture = Self {
            directory,
            component,
            manifest,
            key,
            output,
            publisher,
        };
        fixture.write_manifest("first-app", "revision-1")?;
        Ok(fixture)
    }
    fn write_manifest(&self, id: &str, version: &str) -> Result<()> {
        let bytes = fs::read(&self.component).map_err(|e| e.to_string())?;
        let manifest = ToolManifest {
            id: id.into(),
            version: version.into(),
            summary: "Synthetic fixture".into(),
            needs: Needs::Wasm,
            egress: vec![],
            secrets: vec![],
            component_sha256: Some(sha(&bytes)),
        };
        fs::write(
            &self.manifest,
            serde_json::to_vec(&manifest).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
    fn publish(&self) -> Result<serde_json::Value> {
        publish(
            &self.component,
            &self.manifest,
            &self.key,
            &self.publisher,
            &self.output,
        )
    }
    fn signed(&self) -> Result<SignedManifest> {
        serde_json::from_slice(&fs::read(&self.output).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
    fn unchanged_directory(&self) -> Result<Vec<String>> {
        let mut names = fs::read_dir(self.directory.path())
            .map_err(|e| e.to_string())?
            .map(|entry| entry.map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        names.sort();
        Ok(names)
    }
}

#[test]
fn signs_actual_bytes_for_multiple_apps_and_verifies_expected_publisher() -> Result<()> {
    for (id, version) in [
        ("wordcraft-canonical-v1", "word-revision"),
        ("pdfcraft-canonical-v1", "pdf-revision"),
        ("future-app", "future-revision"),
    ] {
        let fixture = Fixture::new()?;
        fixture.write_manifest(id, version)?;
        let receipt = fixture.publish()?;
        let signed = fixture.signed()?;
        let key = publisher(&fixture.publisher)?;
        let verified = signed.verify(&[key]).map_err(|e| e.to_string())?;
        assert_eq!(verified.id, id);
        assert_eq!(verified.version, version);
        assert_eq!(
            verified.component_sha256.as_deref(),
            Some(sha(&fs::read(&fixture.component).map_err(|e| e.to_string())?).as_str())
        );
        assert_eq!(receipt["registered"], false);
        assert_eq!(receipt["publisher"], fixture.publisher);
    }
    Ok(())
}

#[test]
fn tampered_signed_payload_or_wrong_publisher_is_rejected_by_real_verifier() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.publish()?;
    let mut signed = fixture.signed()?;
    assert!(signed
        .verify(&[SigningKey::from_bytes(&[32; 32]).verifying_key()])
        .is_err());
    signed.manifest.version.push_str("-tampered");
    assert!(signed.verify(&[publisher(&fixture.publisher)?]).is_err());
    Ok(())
}

#[test]
fn component_tamper_invalid_header_or_manifest_never_publishes() -> Result<()> {
    for bad in [
        b"\0asm\x0d\0\x01\0changed".as_slice(),
        b"\0asm\x01\0\0\0".as_slice(),
    ] {
        let fixture = Fixture::new()?;
        fs::write(&fixture.component, bad).map_err(|e| e.to_string())?;
        assert!(fixture.publish().is_err());
        assert!(!fixture.output.exists());
    }
    let fixture = Fixture::new()?;
    fixture.write_manifest("invalid/id", "revision")?;
    assert!(fixture.publish().is_err());
    assert!(!fixture.output.exists());
    Ok(())
}

#[test]
fn publisher_mismatch_and_invalid_or_insecure_keys_never_publish() -> Result<()> {
    let fixture = Fixture::new()?;
    let wrong = hex::encode(SigningKey::from_bytes(&[32; 32]).verifying_key().to_bytes());
    assert!(publish(
        &fixture.component,
        &fixture.manifest,
        &fixture.key,
        &wrong,
        &fixture.output
    )
    .is_err());
    fs::set_permissions(&fixture.key, fs::Permissions::from_mode(0o644))
        .map_err(|e| e.to_string())?;
    assert!(fixture.publish().is_err());
    fs::set_permissions(&fixture.key, fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    fs::write(&fixture.key, b"too short").map_err(|e| e.to_string())?;
    assert!(fixture.publish().is_err());
    assert!(!fixture.output.exists());
    Ok(())
}

#[test]
fn key_symlink_is_rejected_without_reading_target() -> Result<()> {
    let fixture = Fixture::new()?;
    let alias = fixture.directory.path().join("key-alias");
    std::os::unix::fs::symlink(&fixture.key, &alias).map_err(|e| e.to_string())?;
    assert!(publish(
        &fixture.component,
        &fixture.manifest,
        &alias,
        &fixture.publisher,
        &fixture.output
    )
    .is_err());
    assert!(!fixture.output.exists());
    Ok(())
}

#[test]
fn output_collision_and_io_failure_preserve_existing_bytes_and_remove_staging() -> Result<()> {
    let fixture = Fixture::new()?;
    fs::write(&fixture.output, b"existing output").map_err(|e| e.to_string())?;
    let before = fixture.unchanged_directory()?;
    assert!(fixture.publish().is_err());
    assert_eq!(
        fs::read(&fixture.output).map_err(|e| e.to_string())?,
        b"existing output"
    );
    assert_eq!(fixture.unchanged_directory()?, before);
    let unavailable = fixture.directory.path().join("missing/signed.json");
    assert!(publish(
        &fixture.component,
        &fixture.manifest,
        &fixture.key,
        &fixture.publisher,
        &unavailable
    )
    .is_err());
    assert_eq!(fixture.unchanged_directory()?, before);
    fs::remove_file(&fixture.manifest).map_err(|e| e.to_string())?;
    assert!(fixture.publish().is_err());
    Ok(())
}

#[test]
fn refuses_egress_and_non_wasm_without_signing() -> Result<()> {
    for change in [
        json!({"needs":{"kind":"none"}}),
        json!({"egress":[{"host":"example.com","methods":["GET"]}]}),
    ] {
        let fixture = Fixture::new()?;
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&fixture.manifest).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        let fields = change.as_object().ok_or("fixture object")?;
        for (name, replacement) in fields {
            value[name] = replacement.clone();
        }
        fs::write(
            &fixture.manifest,
            serde_json::to_vec(&value).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        assert!(fixture.publish().is_err());
        assert!(!fixture.output.exists());
    }
    Ok(())
}

#[test]
fn invalid_arguments_are_safe_and_do_not_need_a_key() {
    assert!(run(&[]).is_err());
    assert!(publisher("invalid public key").is_err());
}

#[test]
fn signed_serialization_must_fit_the_actual_loader_bound_before_publication() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    value["summary"] = json!("x".repeat(65_050));
    let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    assert!(bytes.len() < 65_536);
    fs::write(&fixture.manifest, bytes).map_err(|e| e.to_string())?;
    let before = fixture.unchanged_directory()?;
    let error = fixture
        .publish()
        .err()
        .ok_or("oversize signed manifest was published")?;
    assert!(error.contains("normal loader"));
    assert!(!fixture.output.exists());
    assert_eq!(fixture.unchanged_directory()?, before);
    Ok(())
}
