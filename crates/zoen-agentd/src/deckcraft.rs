//! Pinned headless DeckCraft commands in the existing signed WASI-p2 tier.
//! This module has no keys, storage or actor authority; its host supplies an
//! already-authorized snapshot and must CAS-save the exact returned proposal.
use crate::{
    manifest::SignedManifest,
    wasm::{WasmLimits, WasmTier, WasmTool},
};
use bytes::Bytes;
use ed25519_dalek::VerifyingKey;
use serde_json::Value;
use std::time::Duration;

pub const DOCUMENT_LIMIT: usize = 512 * 1024;
pub const METADATA_LIMIT: usize = 64 * 1024;
pub const ENGINE_REVISION: &str = "a31b990d32f2c8562f1588562f922960c3fde48b";
pub const WORKER_ID: &str = "deckcraft-canonical-v1";
pub struct DeckcraftWorker {
    tier: WasmTier,
    tool: WasmTool,
}
pub struct DeckcraftReply {
    pub metadata: Value,
    pub bytes: Vec<u8>,
}
impl DeckcraftWorker {
    /// Deployment configuration pins a trusted publisher and an exact signed
    /// component, never a URL/path/tool name chosen by a model or App.
    pub fn load(
        trusted: Vec<VerifyingKey>,
        manifest: &SignedManifest,
        component: &[u8],
    ) -> Result<Self, String> {
        let spec = manifest.verify(&trusted).map_err(|e| e.to_string())?;
        if spec.id != WORKER_ID
            || spec.version != ENGINE_REVISION
            || !spec.egress.is_empty()
            || !spec.secrets.is_empty()
        {
            return Err("Invalid DeckCraft worker manifest".into());
        }
        let tier = WasmTier::new(trusted, 1, 256 * 1024 * 1024).map_err(|e| e.to_string())?;
        let tool = tier.load(manifest, component).map_err(|e| e.to_string())?;
        Ok(Self { tier, tool })
    }
    pub async fn run(&self, metadata: &Value, document: &[u8]) -> Result<DeckcraftReply, String> {
        if document.is_empty() || document.len() > DOCUMENT_LIMIT {
            return Err("DeckCraft isolated executor limit: 512 KiB".into());
        }
        let metadata = serde_json::to_vec(metadata).map_err(|_| "Invalid worker request")?;
        if metadata.is_empty() || metadata.len() > METADATA_LIMIT {
            return Err("DeckCraft command metadata exceeds 64 KiB".into());
        }
        let mut input = Vec::with_capacity(4 + metadata.len() + document.len());
        input.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        input.extend_from_slice(&metadata);
        input.extend_from_slice(document);
        let output = self
            .tier
            .run(
                &self.tool,
                Bytes::from(input),
                WasmLimits {
                    fuel: 2_000_000_000,
                    wall: Duration::from_secs(5),
                    memory_bytes: 256 * 1024 * 1024,
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        if output.stopped.is_some() {
            return Err("The isolated presentation worker did not complete".into());
        }
        decode_reply(&output.stdout)
    }
}
fn decode_reply(bytes: &[u8]) -> Result<DeckcraftReply, String> {
    let prefix = bytes.get(..4).ok_or("Invalid DeckCraft worker frame")?;
    let length =
        u32::from_le_bytes(prefix.try_into().map_err(|_| "Invalid worker prefix")?) as usize;
    if length == 0 || length > METADATA_LIMIT || bytes.len() > 4 + METADATA_LIMIT + DOCUMENT_LIMIT {
        return Err("Oversized DeckCraft worker frame".into());
    }
    let split = 4 + length;
    let metadata: Value =
        serde_json::from_slice(bytes.get(4..split).ok_or("Truncated worker frame")?)
            .map_err(|_| "Invalid worker response")?;
    let body = bytes.get(split..).ok_or("Truncated worker body")?;
    let object = metadata.as_object().ok_or("Invalid worker response")?;
    if object.get("ok").and_then(Value::as_bool) == Some(false) {
        if !body.is_empty()
            || object.len() != 2
            || object.get("error").and_then(Value::as_str).is_none()
        {
            return Err("Invalid worker refusal".into());
        }
        return Err(format!(
            "Presentation worker refused: {}",
            object["error"].as_str().unwrap_or("invalid")
        ));
    }
    if object.len() != 2
        || object.get("ok").and_then(Value::as_bool) != Some(true)
        || body.len() > DOCUMENT_LIMIT
    {
        return Err("Invalid worker response".into());
    }
    let metadata = object
        .get("metadata")
        .filter(|v| v.is_object())
        .ok_or("Invalid worker metadata")?
        .clone();
    if metadata.get("engineRevision").and_then(Value::as_str) != Some(ENGINE_REVISION) {
        return Err("Worker engine revision changed".into());
    }
    Ok(DeckcraftReply {
        metadata,
        bytes: body.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn frame(metadata: Value, body: &[u8]) -> Vec<u8> {
        let encoded = serde_json::to_vec(&metadata).unwrap();
        let mut out = (encoded.len() as u32).to_le_bytes().to_vec();
        out.extend(encoded);
        out.extend(body);
        out
    }
    #[test]
    fn output_frames_require_bounded_complete_metadata_and_exact_pinned_engine() {
        let metadata = json!({"engineRevision":ENGINE_REVISION,"scope":{"artifactId":"a"}});
        let accepted =
            decode_reply(&frame(json!({"ok":true,"metadata":metadata}), b"proposal")).unwrap();
        assert_eq!(accepted.bytes, b"proposal");
        assert_eq!(accepted.metadata["engineRevision"], ENGINE_REVISION);
        let invalid = [
            vec![],
            vec![1, 0, 0],
            vec![9, 0, 0, 0, b'{'],
            ((METADATA_LIMIT + 1) as u32).to_le_bytes().to_vec(),
            frame(
                json!({"ok":true,"metadata":{"engineRevision":"other"}}),
                b"bytes",
            ),
            frame(
                json!({"ok":true,"metadata":{"engineRevision":ENGINE_REVISION},"extra":true}),
                b"bytes",
            ),
            frame(
                json!({"ok":false,"error":"denied"}),
                b"unexpected partial bytes",
            ),
            frame(
                json!({"ok":true,"metadata":{"engineRevision":ENGINE_REVISION}}),
                &vec![0; DOCUMENT_LIMIT + 1],
            ),
        ];
        for (index, bytes) in invalid.iter().enumerate() {
            assert!(decode_reply(bytes).is_err(), "case {index}");
        }
        let error = decode_reply(&frame(json!({"ok":false,"error":"invalid_command"}), b""))
            .err()
            .unwrap();
        assert_eq!(error, "Presentation worker refused: invalid_command");
    }
}
