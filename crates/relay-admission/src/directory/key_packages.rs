//! Native MLS publication validation and exact destructive-claim input policy.

pub const MAX_PACKAGE_BYTES: usize = 4096;
pub const MAX_PUBLICATIONS: usize = 100;
pub const MAX_STOCK: usize = 200;
pub const MAX_RECEIPTS: i64 = 4096;
pub const MAX_RECEIPT_BYTES: i64 = 8 * 1024 * 1024;

/// # Errors
/// Refuses excessive, oversized or invalid packages, invalid expiry and packages not signed for this identity/device.
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
        let (leaf, expires) = mls::key_package_publication(bytes)
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
/// # Errors
/// Refuses malformed operation timestamps, noncanonical identity keys or a target count outside one to fifty after deduplication.
pub fn claim_input(operation: &str, targets: &[String]) -> Result<Claim, &'static str> {
    let created = protocol::key_package_claim_time_ms(operation)
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
    let expires_ms = created + protocol::KEY_PACKAGE_CLAIM_TTL_MS;
    Ok(Claim {
        targets,
        created_ms: created,
        expires_ms,
    })
}
/// # Errors
/// Refuses future-created or expired key-package claim operations.
pub fn claim_time(input: &Claim, now: i64) -> Result<(), &'static str> {
    if input.created_ms > now {
        return Err("key-package claim operation is in the future");
    }
    if input.expires_ms <= now {
        return Err(protocol::KEY_PACKAGE_CLAIM_EXPIRED);
    }
    Ok(())
}
