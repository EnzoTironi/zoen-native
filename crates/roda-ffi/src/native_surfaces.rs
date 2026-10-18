//! One owner-bound, consistent read for native surfaces. No full app views or storage.
use std::collections::{BTreeSet, HashSet};
use std::io::{self, Write};

use crate::engine::{now_ms, Engine, R};
use crate::i18n::t;
use crate::{apps, CoreError, NativeSurfaceAppDto, NativeSurfaceItemDto, RodaEngine};
use roda_types::{ItemContent, ItemKind};
use serde_json::Value;

const MAX_ITEMS: usize = 64;
const MAX_SNAPSHOT_BYTES: usize = 16 * 1024;

fn valid_id(value: &str, maximum: usize, owner: bool) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || (owner && b == b':'))
}

fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: t(
            "seleção de mini-apps inválida ou grande demais",
            "invalid or oversized mini-app selection",
        ),
    }
}

fn forbidden() -> CoreError {
    CoreError::Forbidden {
        reason: t(
            "a conta ou o acesso deste aparelho mudou",
            "the account or this device's access changed",
        ),
    }
}

fn validate_request(identity: &str, device: &str, ids: &[String]) -> R<()> {
    if !valid_id(identity, 256, true)
        || !valid_id(device, 256, true)
        || ids.len() > MAX_ITEMS
        || ids.iter().any(|id| !valid_id(id, 128, false))
    {
        return Err(invalid());
    }
    Ok(())
}

/// Serialization stops at the bridge limit instead of building an oversized string.
#[derive(Default)]
struct SnapshotBytes(Vec<u8>);
impl Write for SnapshotBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_SNAPSHOT_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("native snapshot exceeds its limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn snapshot_json(app: &str, state: &Value, item: &str, now: i64) -> Option<String> {
    // This is exactly the canonical snapshot producer used by app_state_dto.
    let snapshot = apps::snapshot(app, state, item, now)?;
    if !matches!(
        snapshot["template"].as_str(),
        Some("stat" | "progress" | "countdown" | "list" | "caption" | "ticket" | "photo")
    ) || snapshot["title"]
        .as_str()
        .is_none_or(|title| title.trim().is_empty())
    {
        return None;
    }
    let mut bytes = SnapshotBytes::default();
    serde_json::to_writer(&mut bytes, &snapshot).ok()?;
    String::from_utf8(bytes.0).ok()
}

impl Engine {
    fn native_surface_owner(&self, identity: &str, device: &str) -> R<()> {
        let account = self.net.account.as_ref().ok_or_else(forbidden)?;
        let author = self.net.author.as_ref().ok_or_else(forbidden)?;
        if account.identity != identity
            || account.device != device
            || self.me.as_deref() != Some(identity)
            || self.provider_setup_pending()
            || author.identity != identity
            || author.device.as_deref() != Some(device)
            || author.key.id() != device
            || author.cert.as_deref() != Some(account.cert.as_str())
            || !roda_log::verify_sig(
                identity,
                &roda_log::device_cert_message(device),
                &account.cert,
            )
        {
            return Err(forbidden());
        }
        // Do not interpret a corrupt revocation cache as an empty set.
        let revoked: BTreeSet<String> = match self.store.meta("devices.revoked")? {
            None => BTreeSet::new(),
            Some(json) => serde_json::from_str(&json).map_err(|_| CoreError::Storage {
                message: t(
                    "não foi possível verificar o acesso deste aparelho",
                    "could not verify this device's access",
                ),
            })?,
        };
        if revoked.contains(device) {
            return Err(forbidden());
        }
        Ok(())
    }

    fn native_surface_items(
        &self,
        identity: &str,
        device: &str,
        ids: &[String],
    ) -> R<Vec<NativeSurfaceItemDto>> {
        self.native_surface_owner(identity, device)?;
        let mut seen = HashSet::with_capacity(ids.len());
        let now = now_ms();
        Ok(ids
            .iter()
            .filter(|id| seen.insert(id.as_str()))
            .filter_map(|id| {
                // The merged index excludes ambiguous cross-space item IDs. Content and
                // membership must then come from that exact Space's current projection.
                let indexed = self.state.items.get(id)?;
                if !valid_id(&indexed.space, 128, false)
                    || self.broken_spaces.contains_key(&indexed.space)
                {
                    return None;
                }
                let scoped = self.space_projections.get(&indexed.space)?;
                let space = scoped.spaces.get(&indexed.space)?;
                if space.integrity_error.is_some()
                    || !space.members.iter().any(|(member, _)| member == identity)
                {
                    return None;
                }
                let item = scoped.items.get(id)?;
                let version = item.versions.last()?;
                if item.id != *id
                    || item.space != indexed.space
                    || item.kind != ItemKind::App
                    || version.number == 0
                {
                    return None;
                }
                let ItemContent::App(doc) = &version.content else {
                    return None;
                };
                if doc.title.len() > MAX_SNAPSHOT_BYTES {
                    return None;
                }
                let state: Value = serde_json::from_str(&doc.state_json).ok()?;
                if !state.is_object() {
                    return None;
                }
                Some(NativeSurfaceItemDto {
                    id: item.id.clone(),
                    space_id: item.space.clone(),
                    title: doc.title.clone(),
                    version: version.number,
                    app: NativeSurfaceAppDto {
                        app_id: doc.app.clone(),
                        snapshot_json: snapshot_json(&doc.app, &state, &item.id, now)?,
                    },
                })
            })
            .collect())
    }
}

#[uniffi::export]
impl RodaEngine {
    /// Missing/inaccessible items are omitted so callers clear stale publications.
    pub fn native_surface_items(
        &self,
        expected_identity_id: String,
        expected_device_id: String,
        item_ids: Vec<String>,
    ) -> Result<Vec<NativeSurfaceItemDto>, CoreError> {
        validate_request(&expected_identity_id, &expected_device_id, &item_ids)?;
        self.lock()
            .native_surface_items(&expected_identity_id, &expected_device_id, &item_ids)
    }
}

#[cfg(test)]
mod tests;
