//! Shared MLS validation, destructive-claim policy and canonical replies.
use super::{internal, no, Session};
use crate::portable::BoundaryError;
use base64::{engine::general_purpose::STANDARD, Engine};
use roda_proto::{KeyPackageRecord, Reply, ServerFrame};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const MAX_PACKAGE_BYTES: usize = 4096;
pub const MAX_PUBLICATIONS: usize = 100;
pub const MAX_STOCK: usize = 200;
pub const MAX_RECEIPTS: i64 = 4096;
pub const MAX_RECEIPT_BYTES: i64 = 8 * 1024 * 1024;
// Explicit private-adapter bounds, distinct from native aggregate quotas.
pub const PRIVATE_MAX_RECORDS: usize = 128;
pub const PRIVATE_MAX_RESPONSE_BYTES: usize = 768 * 1024;

pub fn validate_publications(
    identity: &str,
    device: &str,
    packages: &[Vec<u8>],
    last_resort: Option<&[u8]>,
) -> Result<Vec<(Vec<u8>, i64)>, String> {
    if packages.len() > MAX_PUBLICATIONS {
        return Err("at most 100 key packages at a time".into());
    }
    let validate = |bytes: &[u8]| {
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err("key package too large".to_string());
        }
        let (leaf, expires) = roda_mls::key_package_publication(bytes)
            .map_err(|_| "not a valid key package".to_string())?;
        if leaf.identity != identity || leaf.device != device {
            return Err("key package is for another device".into());
        }
        Ok(expires)
    };
    let mut validated = Vec::with_capacity(packages.len());
    for bytes in packages {
        validated.push((bytes.clone(), validate(bytes)?));
    }
    if let Some(bytes) = last_resort {
        validate(bytes)?;
    }
    Ok(validated)
}
#[derive(Clone, Debug)]
pub struct Claim {
    pub targets: Vec<String>,
    pub created_ms: i64,
    pub expires_ms: i64,
}
/// Exact native receipt-mode validation and error order.
pub fn claim_input(operation: &str, targets: &[String]) -> Result<Claim, &'static str> {
    let created = roda_proto::key_package_claim_time_ms(operation)
        .ok_or("invalid key-package claim operation")?;
    if targets
        .iter()
        .any(|id| id.len() != 64 || !id.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')))
    {
        return Err("claim targets must be canonical identity keys");
    }
    let mut targets = targets.to_vec();
    targets.sort_unstable();
    targets.dedup();
    if targets.is_empty() || targets.len() > 50 {
        return Err("claim one to fifty identities");
    }
    let expires_ms = created + roda_proto::KEY_PACKAGE_CLAIM_TTL_MS;
    Ok(Claim {
        targets,
        created_ms: created,
        expires_ms,
    })
}
pub fn claim_time(input: &Claim, now: i64) -> Result<(), &'static str> {
    if input.created_ms > now {
        return Err("key-package claim operation is in the future");
    }
    if input.expires_ms <= now {
        return Err(roda_proto::KEY_PACKAGE_CLAIM_EXPIRED);
    }
    Ok(())
}
pub fn normalize_claim(
    operation: &str,
    targets: &[String],
    now: i64,
) -> Result<Claim, &'static str> {
    let input = claim_input(operation, targets)?;
    claim_time(&input, now)?;
    Ok(input)
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Facts {
    pub now_ms: String,
    pub publication_hashes: Vec<String>,
    pub stock: usize,
    pub receipt: Option<Receipt>,
    pub receipt_count: u32,
    pub receipt_bytes: String,
    pub selected: Vec<Candidate>,
    pub oversized: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub targets: Vec<String>,
    pub response_base64: String,
    pub expires_ms: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub id: String,
    pub identity: String,
    pub device: String,
    pub data_base64: String,
    pub last_resort: bool,
}
fn capacity() -> BoundaryError {
    BoundaryError::internal(
        "private_key_package_capacity",
        "key-package claim exceeds private adapter capacity",
    )
}
fn now(facts: &Facts) -> Result<i64, BoundaryError> {
    facts
        .now_ms
        .parse::<i64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| internal("invalid key-package clock"))
}
pub fn publications(
    session: &Session,
    packages: &[Vec<u8>],
    last_resort: Option<&[u8]>,
    facts: &Facts,
) -> Result<Value, BoundaryError> {
    let validated =
        validate_publications(&session.identity, &session.device, packages, last_resort)
            .map_err(no)?;
    let now = now(facts)?;
    let mut known: BTreeSet<_> = facts.publication_hashes.iter().cloned().collect();
    let mut fresh = Vec::new();
    for (bytes, expires) in validated {
        let hash = roda_log::sha256_hex(&bytes);
        if known.insert(hash.clone()) {
            fresh.push(json!({"package_hash":hash,"data_base64":STANDARD.encode(bytes),"expires_secs":expires.to_string()}));
        }
    }
    if facts
        .stock
        .checked_add(fresh.len())
        .is_none_or(|count| count > MAX_STOCK)
    {
        return Err(no("too many key packages stored"));
    }
    Ok(
        json!({"action":"publish_key_packages","publications":fresh,"last_resort_base64":last_resort.map(|bytes| STANDARD.encode(bytes)),"cleanup_before_secs":(now/1000).to_string()}),
    )
}
pub fn decode_reply(bytes: &[u8]) -> Result<Vec<KeyPackageRecord>, BoundaryError> {
    if bytes.len() > PRIVATE_MAX_RESPONSE_BYTES {
        return Err(capacity());
    }
    let frame =
        ServerFrame::decode(bytes).map_err(|_| internal("corrupt key-package claim receipt"))?;
    if frame.encode() != bytes {
        return Err(internal("noncanonical key-package claim receipt"));
    }
    match frame {
        ServerFrame::Res {
            result: Ok(Reply::KeyPackages(records)),
            ..
        } => Ok(records),
        _ => Err(internal("corrupt key-package claim receipt")),
    }
}
pub fn claim(
    ids: &[String],
    operation: Option<&str>,
    facts: &Facts,
) -> Result<(Value, Vec<KeyPackageRecord>), BoundaryError> {
    let now = now(facts)?;
    let normalized = operation
        .map(|op| normalize_claim(op, ids, now))
        .transpose()
        .map_err(no)?;
    let targets = normalized
        .as_ref()
        .map(|value| value.targets.clone())
        .unwrap_or_else(|| ids.iter().take(50).cloned().collect());
    if let (Some(operation), Some(normalized), Some(receipt)) =
        (operation, &normalized, &facts.receipt)
    {
        if receipt.targets != targets {
            return Err(no(
                "key-package claim operation reused with different targets",
            ));
        }
        if receipt.expires_ms != normalized.expires_ms.to_string() {
            return Err(internal("key-package receipt expiry mismatch"));
        }
        let bytes = STANDARD
            .decode(&receipt.response_base64)
            .map_err(|_| internal("invalid key-package receipt bytes"))?;
        let records = decode_reply(&bytes)?;
        if !matches!(
            ServerFrame::decode(&bytes),
            Ok(ServerFrame::Res { id: 0, .. })
        ) {
            return Err(internal("corrupt key-package claim receipt"));
        }
        return Ok((
            json!({"action":"claim_key_packages","delete_ids":[],"receipt":null,"operation_id":operation,"cleanup_before_ms":now.to_string(),"replayed":true}),
            records,
        ));
    }
    let stored_bytes = facts
        .receipt_bytes
        .parse::<i64>()
        .ok()
        .filter(|value| *value >= 0)
        .ok_or_else(|| internal("invalid key-package receipt accounting"))?;
    if operation.is_some()
        && (i64::from(facts.receipt_count) >= MAX_RECEIPTS || stored_bytes >= MAX_RECEIPT_BYTES)
    {
        return Err(no("key-package claim receipt capacity reached"));
    }
    if facts.oversized || facts.selected.len() > PRIVATE_MAX_RECORDS {
        return Err(capacity());
    }
    let mut records = Vec::with_capacity(facts.selected.len());
    let mut delete = Vec::new();
    let mut devices = BTreeSet::new();
    for selected in &facts.selected {
        if !targets.contains(&selected.identity)
            || !devices.insert((&selected.identity, &selected.device))
        {
            return Err(internal(
                "key-package selection has wrong scope or duplicate device",
            ));
        }
        if selected.id.parse::<i64>().ok().is_none_or(|id| id <= 0) {
            return Err(internal("invalid key-package row id"));
        }
        let data = STANDARD
            .decode(&selected.data_base64)
            .map_err(|_| internal("invalid stored key-package bytes"))?;
        if data.len() > MAX_PACKAGE_BYTES {
            return Err(capacity());
        }
        if !selected.last_resort {
            delete.push(selected.id.clone());
        }
        records.push(KeyPackageRecord {
            identity: selected.identity.clone(),
            device: selected.device.clone(),
            data,
        });
    }
    let response = ServerFrame::Res {
        id: 0,
        result: Ok(Reply::KeyPackages(records.clone())),
    }
    .encode();
    if response.len() > PRIVATE_MAX_RESPONSE_BYTES {
        return Err(capacity());
    }
    if operation.is_some() && stored_bytes + response.len() as i64 > MAX_RECEIPT_BYTES {
        return Err(no("key-package claim receipt capacity reached"));
    }
    let receipt = normalized.map(|value| json!({"targets":value.targets,"response_base64":STANDARD.encode(response),"expires_ms":value.expires_ms.to_string()}));
    Ok((
        json!({"action":"claim_key_packages","delete_ids":delete,"receipt":receipt,"operation_id":operation,"cleanup_before_ms":now.to_string(),"replayed":false}),
        records,
    ))
}
pub fn reply_json(bytes: &[u8]) -> Result<String, BoundaryError> {
    let records = decode_reply(bytes)?;
    serde_json::to_string(&records.into_iter().map(|record| json!({"identity":record.identity,"device":record.device,"data_base64":STANDARD.encode(record.data)})).collect::<Vec<_>>()).map_err(|_| internal("key-package reply encoding failed"))
}
