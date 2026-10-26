//! Inspectable Plan operations. This codec is isolated from production PageContent.

use loro::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{error, PlanError, PlanUpdate, Result};

mod atoms;
mod json;
mod operations;
mod validation;

use atoms::atoms;
use json::strict_json;
pub(super) use json::{bounded_json, strict_value};
pub(super) use validation::validate;

/// Matches the existing Page observed-context byte ceiling. Inline/blob selection is separate.
pub const MAX_PLAN_PACKET_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_PLAN_CHANGES: usize = 4_096;
pub const MAX_PLAN_RAW_OPS: usize = 32_768;
pub const MAX_PLAN_ATOMS: usize = 65_536;
const FORMAT: &str = "zoen.plan.loro.json";
const VERSION: u8 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format: String,
    version: u8,
    operations: JsonSchema,
}

#[derive(Serialize)]
struct EncodedEnvelope<'a> {
    format: &'static str,
    version: u8,
    operations: &'a JsonSchema,
}

pub(super) fn decode(bytes: &[u8]) -> Result<PlanUpdate> {
    if bytes.len() > MAX_PLAN_PACKET_BYTES {
        return Err(PlanError::Limit("plan packet bytes"));
    }
    // Parse bounded bytes before handing any input to Loro. Value also lets us reject
    // unknown/ignored fields instead of accepting a different operation interpretation.
    let raw = strict_json(bytes)?;
    let envelope: Envelope = serde_json::from_value(raw.clone()).map_err(error)?;
    if envelope.format != FORMAT || envelope.version != VERSION {
        return Err(PlanError::UnsupportedCodec);
    }
    if serde_json::to_value(&envelope).map_err(error)? != raw {
        return Err(error("noncanonical or unknown plan codec fields"));
    }
    let update = PlanUpdate(envelope.operations);
    atoms(&update.0)?;
    Ok(update)
}

pub(super) fn encode(update: &PlanUpdate) -> Result<Vec<u8>> {
    let bytes = serialized(update)?;
    atoms(&update.0)?;
    Ok(bytes)
}

pub(super) fn fingerprint(update: &PlanUpdate) -> Result<[u8; 32]> {
    let _ = serialized(update)?;
    let mut digest = Sha256::new().chain_update(b"zoen-plan-operations-v1\0");
    for (id, atom) in atoms(&update.0)? {
        let provenance = atom
            .reserved_provenance
            .as_ref()
            .map(|(timestamp, message)| (*timestamp, message.as_deref()));
        let bytes = serde_json::to_vec(&(
            id,
            atom.container,
            atom.content,
            atom.deps,
            atom.lamport,
            provenance,
        ))
        .map_err(error)?;
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    Ok(digest.finalize().into())
}

fn serialized(update: &PlanUpdate) -> Result<Vec<u8>> {
    let envelope = EncodedEnvelope {
        format: FORMAT,
        version: VERSION,
        operations: &update.0,
    };
    let bytes = bounded_json(&envelope)?;
    strict_json(&bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests;
