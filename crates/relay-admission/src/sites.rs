//! Pure admission only. The host must additionally hold the current human
//! owner's device fence through the receipt/head transaction. Source Item
//! permission and selected-version verification belong to the encrypted client.

use protocol::sites::{Change, SignedInspection, SignedMutation, MAX_HTML_BYTES, SCHEMA};

const MAX_SKEW_MS: u64 = 5 * 60 * 1000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Invalid,
    Signature,
    Stale,
    Content,
}

pub fn valid_sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn valid_site(value: &str) -> bool {
    secret_id(value, "site_")
}

fn secret_id(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn valid_item_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn authorize(signed: &SignedMutation, audience: &str, now_ms: i64) -> Result<(), Failure> {
    let mutation = &signed.mutation;
    if audience.is_empty()
        || audience.len() > 2048
        || mutation.schema != SCHEMA
        || mutation.public_origin.is_empty()
        || mutation.public_origin.len() > 2048
        || !valid_site(&mutation.site_id)
        || !secret_id(&mutation.request_id, "pub_")
        || !valid_sha(&signed.owner_id)
        || !valid_sha(&signed.device_id)
        || mutation
            .expected_head
            .as_deref()
            .is_some_and(|v| !valid_sha(v))
        || matches!(mutation.change, Change::Unpublish) && mutation.expected_head.is_none()
    {
        return Err(Failure::Invalid);
    }
    if let Change::Publish { source } = &mutation.change {
        if !valid_item_id(&source.space_id)
            || !valid_item_id(&source.item_id)
            || source.version == 0
            || !valid_sha(&source.sha256)
            || !(1..=MAX_HTML_BYTES).contains(&source.byte_length)
        {
            return Err(Failure::Invalid);
        }
    }
    if signed.at_ms <= 0 || now_ms <= 0 || now_ms.abs_diff(signed.at_ms) > MAX_SKEW_MS {
        return Err(Failure::Stale);
    }
    if !event_log::verify_sig(
        &signed.device_id,
        &signed.signing_bytes(audience),
        &signed.signature,
    ) {
        return Err(Failure::Signature);
    }
    Ok(())
}

pub fn authorize_inspection(
    signed: &SignedInspection,
    audience: &str,
    now_ms: i64,
) -> Result<(), Failure> {
    if audience.is_empty()
        || audience.len() > 2048
        || !valid_sha(&signed.owner_id)
        || !valid_sha(&signed.device_id)
        || !valid_site(&signed.site_id)
        || signed.public_origin.is_empty()
        || signed.public_origin.len() > 2048
        || signed.before.as_deref().is_some_and(|v| !valid_sha(v))
    {
        return Err(Failure::Invalid);
    }
    if signed.at_ms <= 0 || now_ms <= 0 || now_ms.abs_diff(signed.at_ms) > MAX_SKEW_MS {
        return Err(Failure::Stale);
    }
    if !event_log::verify_sig(
        &signed.device_id,
        &signed.signing_bytes(audience),
        &signed.signature,
    ) {
        return Err(Failure::Signature);
    }
    Ok(())
}

/// Returns the immutable public object key only after exact byte verification.
/// HTML stays opaque: transforming/sanitizing it would publish a different file.
pub fn verify_content(
    signed: &SignedMutation,
    html: Option<&[u8]>,
) -> Result<Option<String>, Failure> {
    match (&signed.mutation.change, html) {
        (Change::Unpublish, None) => Ok(None),
        (Change::Publish { source }, Some(bytes))
            if !bytes.is_empty()
                && valid_site(&signed.mutation.site_id)
                && bytes.len() as u64 == source.byte_length
                && bytes.len() as u64 <= MAX_HTML_BYTES
                && !bytes.contains(&0)
                && std::str::from_utf8(bytes).is_ok()
                && event_log::sha256_hex(bytes) == source.sha256 =>
        {
            Ok(Some(format!(
                "public-sites/v1/{}/{}",
                signed.mutation.site_id, source.sha256
            )))
        }
        _ => Err(Failure::Content),
    }
}

#[cfg(test)]
mod tests;
