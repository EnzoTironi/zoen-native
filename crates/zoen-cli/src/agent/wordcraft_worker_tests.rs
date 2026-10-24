//! Real signed component + certified native agent + controlled Gate/brain; no provider login.
use super::*;
use ed25519_dalek::SigningKey;
use sha2::Digest;
use zoen_agentd::{Needs, ToolManifest};
pub(super) fn worker() -> Arc<zoen_agentd::deckcraft::DeckcraftWorker> {
    let path = std::env::var("ZOEN_TEST_WORDCRAFT_COMPONENT")
        .expect("explicit genuine WordCraft WASI-p2 component required");
    let bytes = std::fs::read(path).expect("actual component");
    assert!(!bytes.is_empty() && bytes.len() <= 64 * 1024 * 1024);
    let key = SigningKey::from_bytes(&[29; 32]);
    let signed = ToolManifest {
        id: "wordcraft-canonical-v1".into(),
        version: "d1b0bfc3f9dd83126b162a42b667facecb003b7f".into(),
        summary: "Pinned actual WordCraft offline fixture".into(),
        needs: Needs::Wasm,
        egress: vec![],
        secrets: vec![],
        component_sha256: Some(hex::encode(sha2::Sha256::digest(&bytes))),
    }
    .sign(&key);
    let mut tampered = bytes.clone();
    tampered.push(0);
    assert!(zoen_agentd::deckcraft::DeckcraftWorker::load_for(
        "wordcraft",
        vec![key.verifying_key()],
        &signed,
        &tampered
    )
    .is_err());
    Arc::new(
        zoen_agentd::deckcraft::DeckcraftWorker::load_for(
            "wordcraft",
            vec![key.verifying_key()],
            &signed,
            &bytes,
        )
        .expect("signed genuine worker"),
    )
}
