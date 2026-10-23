//! Demand operations on the original Engine state.

use super::*;

impl Engine {
    /// A demand never inherits authority from JS or an old account. Recheck the
    /// exact canonical manifest and current read membership on every worker pass.
    pub(crate) fn file_demand_key(
        &self,
        item: &str,
        version: u32,
        max_bytes: u64,
    ) -> R<crate::file_demand::FileDemandKey> {
        let account = self
            .account()
            .filter(|account| {
                self.net.author.as_ref().is_some_and(|author| {
                    author.identity == account.identity
                        && author.device.as_deref() == Some(account.device.as_str())
                }) && self.me.as_deref() == Some(account.identity.as_str())
                    && !self.provider_setup_pending()
            })
            .ok_or_else(|| CoreError::Forbidden {
                reason: t(
                    "Entre novamente para baixar este arquivo.",
                    "Sign in again to download this file.",
                ),
            })?;
        let state = self.item_state(item)?;
        if !self.is_member(&state.space, &account.identity) {
            return Err(CoreError::Forbidden {
                reason: t(
                    "Você não tem mais acesso a esta conversa.",
                    "You no longer have access to this chat.",
                ),
            });
        }
        let file = self.file_doc_at(item, Some(version))?;
        if max_bytes == 0 || max_bytes > MAX_FILE_BYTES as u64 {
            return Err(invalid_file());
        }
        validate_file(file, max_bytes)?;
        Ok(crate::file_demand::FileDemandKey {
            item: item.into(),
            version,
            sha256: file.sha256.clone(),
            identity: account.identity.clone(),
            device: account.device.clone(),
        })
    }

    pub(crate) fn file_demand_info(
        &self,
        key: &crate::file_demand::FileDemandKey,
        max_bytes: u64,
    ) -> R<FileDto> {
        if self.file_demand_key(&key.item, key.version, max_bytes)? != *key {
            return Err(CoreError::Stale {
                reason: t(
                    "Este arquivo mudou. Abra a versão novamente.",
                    "This file changed. Open the version again.",
                ),
            });
        }
        self.file_info_at(&key.item, Some(key.version))
    }

    pub(super) fn demanded_file_blobs(&self) -> Vec<Wanted> {
        self.net
            .file_demands
            .active()
            .into_iter()
            .flat_map(|(key, bound)| self.file_demand_blobs(&key, bound))
            .collect()
    }

    /// A foreground open is a bounded fresh attempt, not evidence of a human
    /// button press. Initial loads use the same API; background snapshots do not.
    pub(crate) fn take_foreground_file_attempts(&self) -> std::collections::HashSet<Wanted> {
        let mut out = std::collections::HashSet::new();
        for (key, bound) in self.net.file_demands.foreground_attempts() {
            let wanted = self.file_demand_blobs(&key, bound);
            // Unsynced demands retain their pulse until they can be admitted.
            if !wanted.is_empty() && self.net.file_demands.consume_foreground_attempt(&key) {
                out.extend(wanted);
            }
        }
        out
    }

    pub(super) fn file_demand_blobs(
        &self,
        key: &crate::file_demand::FileDemandKey,
        bound: u64,
    ) -> Vec<Wanted> {
        let mut out = Vec::new();
        let Ok(info) = self.file_demand_info(key, bound) else {
            self.net.file_demands.remove(key);
            return out;
        };
        if info.ready {
            self.net.file_demands.remove(key);
            return out;
        }
        let item = &self.state.items[&key.item];
        if !self.net.synced.contains(&item.space) {
            return out;
        }
        let file = self
            .file_doc_at(&key.item, Some(key.version))
            .expect("validated canonical file");
        for chunk in &file.chunks {
            if self.store.has_media(&chunk.sha256).unwrap_or(false) {
                continue;
            }
            out.push(Wanted {
                plain: chunk.sha256.clone(),
                blob: chunk.blob.clone(),
                key: chunk.key.clone(),
                mime: "application/octet-stream".into(),
                space: item.space.clone(),
                profile: None,
                maximum_sealed_bytes: Some(chunk.bytes + 40),
            });
        }
        out
    }

    /// Chunks, previews and page blobs that relay-synced Spaces show and this device lacks.
    pub(crate) fn wanted_item_blobs(&self) -> Vec<Wanted> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for it in self.state.items.values() {
            if !self.net.synced.contains(&it.space) {
                continue;
            }
            if matches!(&it.current().content, ItemContent::App(_))
                && !self
                    .me
                    .as_deref()
                    .is_some_and(|me| self.is_member(&it.space, me))
            {
                continue;
            }
            let mut refs: Vec<(&ChunkRef, &str)> = Vec::new();
            match it.versions.last().map(|v| &v.content) {
                Some(content) if content.owned_file().is_some() => {
                    let f = content.owned_file().expect("matched owned file");
                    if validate_file(f, MAX_FILE_BYTES as u64).is_err() {
                        continue;
                    }
                    if let Some(t) = &f.thumb {
                        refs.push((t, "image/png"));
                    }
                    for c in &f.chunks {
                        refs.push((c, "application/octet-stream"));
                    }
                }
                Some(ItemContent::Page(_)) => {
                    for v in &it.versions {
                        if let ItemContent::Page(p) = &v.content {
                            if let Some(b) = &p.blob {
                                refs.push((b, "application/vnd.zoen.page"));
                            }
                        }
                    }
                }
                _ => {}
            }
            for (c, mime) in refs {
                let Some(maximum_sealed_bytes) = c.bytes.checked_add(40) else {
                    continue;
                };
                if !seen.insert(c.sha256.clone()) {
                    continue;
                }
                if self.store.has_media(&c.sha256).unwrap_or(false) {
                    continue;
                }
                out.push(Wanted {
                    plain: c.sha256.clone(),
                    blob: c.blob.clone(),
                    key: c.key.clone(),
                    mime: mime.into(),
                    space: it.space.clone(),
                    profile: None,
                    maximum_sealed_bytes: Some(maximum_sealed_bytes),
                });
            }
        }
        // Explicit, expiring historical requests only; unrelated history stays lazy.
        for wanted in self.demanded_file_blobs() {
            if seen.insert(wanted.plain.clone()) {
                out.push(wanted);
            }
        }
        out
    }

    /// Seals a page payload too big for an event as an encrypted blob.
    pub(crate) fn page_blob(&mut self, space: &str, bytes: &[u8]) -> R<ChunkRef> {
        let key = random_key()?;
        self.put_chunk(
            space,
            &key,
            bytes,
            "application/vnd.zoen.page",
            &HashMap::new(),
        )
    }
}
