//! Operator-only, offline publication of a managed App's existing `WASIp2` worker.
//! No account, trust, provider, artifact permission or runtime registration is changed.
#![deny(
    clippy::pedantic,
    clippy::perf,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

#[path = "app_worker_publish/files.rs"]
mod files;
#[cfg(all(test, unix))]
#[path = "app_worker_publish/tests.rs"]
mod tests;

use ed25519_dalek::{SigningKey, VerifyingKey};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{path::Path, process::ExitCode};
use zoen_agentd::{Needs, SignedManifest, ToolManifest};

type Result<T> = std::result::Result<T, String>;
const COMPONENT_LIMIT: u64 = 128 * 1024 * 1024;
const MANIFEST_LIMIT: u64 = 64 * 1024;
const USAGE: &str = "usage: app_worker_publish COMPONENT.wasm UNSIGNED.json KEY_FILE EXPECTED_PUBLIC_HEX NEW_SIGNED.json\nKEY_FILE: existing operator's raw 32-byte seed, regular non-symlink file, mode 0600; never pass a secret as an argument. Output must not exist.";

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn publisher(value: &str) -> Result<VerifyingKey> {
    let bytes: [u8; 32] = hex::decode(value)
        .map_err(|_| "expected publisher must be 64 hexadecimal characters")?
        .try_into()
        .map_err(|_| "expected publisher must be 32 bytes")?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| "invalid expected publisher".into())
}

fn signing_key(path: &Path) -> Result<SigningKey> {
    let seed = files::read_key(path)?;
    Ok(SigningKey::from_bytes(&seed))
}

fn publish(
    component_path: &Path,
    manifest_path: &Path,
    key_path: &Path,
    expected: &str,
    output: &Path,
) -> Result<serde_json::Value> {
    let expected = publisher(expected)?;
    let component = files::read(component_path, COMPONENT_LIMIT, "component")?;
    if !component.starts_with(b"\0asm\x0d\0\x01\0") {
        return Err("component must have the WASIp2 component-model header".into());
    }
    let manifest_bytes = files::read(manifest_path, MANIFEST_LIMIT, "unsigned manifest")?;
    let manifest: ToolManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| "invalid unsigned manifest JSON")?;
    manifest.validate().map_err(|e| e.to_string())?;
    if manifest.needs != Needs::Wasm || !manifest.egress.is_empty() || !manifest.secrets.is_empty()
    {
        return Err("managed App worker requires needs=wasm and empty egress/secrets".into());
    }
    let component_sha256 = sha(&component);
    if manifest.component_sha256.as_deref() != Some(&component_sha256) {
        return Err("manifest component_sha256 does not match the actual component bytes".into());
    }
    let key = signing_key(key_path)?;
    if key.verifying_key() != expected {
        return Err("key does not match the expected publisher".into());
    }
    let signed = manifest.sign(&key);
    signed
        .verify(&[expected])
        .map_err(|e| format!("publication verification failed: {e}"))?;
    let mut serialized = serde_json::to_vec_pretty(&signed).map_err(|_| "signed manifest JSON")?;
    serialized.push(b'\n');
    if u64::try_from(serialized.len()).map_err(|_| "signed manifest length")? > MANIFEST_LIMIT {
        return Err("signed manifest exceeds the normal loader's 64 KiB limit".into());
    }
    // Verify the actual serialized output before any publication, using the normal consumer API.
    let roundtrip: SignedManifest =
        serde_json::from_slice(&serialized).map_err(|_| "signed manifest roundtrip")?;
    roundtrip.verify(&[expected]).map_err(|e| e.to_string())?;
    let output = files::publish_new(output, &serialized)?;
    Ok(json!({
        "output":output,"publisher":signed.publisher,"id":signed.manifest.id,
        "version":signed.manifest.version,"componentBytes":component.len(),
        "componentSha256":component_sha256,"signedManifestSha256":sha(&serialized),
        "registered":false
    }))
}

fn run(args: &[String]) -> Result<serde_json::Value> {
    let [component, manifest, key, expected, output] = args else {
        return Err(USAGE.into());
    };
    publish(
        Path::new(component),
        Path::new(manifest),
        Path::new(key),
        expected,
        Path::new(output),
    )
}

fn main() -> ExitCode {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(receipt) => {
            println!("{receipt}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("app_worker_publish: {error}");
            ExitCode::FAILURE
        }
    }
}
