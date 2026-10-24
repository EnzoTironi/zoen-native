//! Capture on the original Engine and canonical document state.

use super::*;

impl Engine {
    pub(super) fn media_draft(&self, scope: &MediaDraftScope) -> R<MediaDraftDto> {
        let storage = self.media_draft_scope(scope)?;
        self.load_media_draft(&storage)?.dto()
    }

    pub(super) fn save_media_draft(
        &mut self,
        scope: &MediaDraftScope,
        expected: &str,
        json: &str,
        inputs: Vec<MediaDraftAssetInput>,
    ) -> R<MediaDraftSaveResult> {
        self.save_media_draft_with_recent_sources(scope, expected, json, inputs, Vec::new())
    }

    pub(super) fn save_media_draft_with_recent_sources(
        &mut self,
        scope: &MediaDraftScope,
        expected: &str,
        json: &str,
        inputs: Vec<MediaDraftAssetInput>,
        mut source_bindings: Vec<crate::RecentAttachmentSourceBinding>,
    ) -> R<MediaDraftSaveResult> {
        let storage = self.media_draft_scope(scope)?;
        let previous = self.load_media_draft(&storage)?;
        if !intent_id(expected) {
            return Err(stale());
        }
        if json.len() > MAX_CONTENT_BYTES || inputs.len() > 10 || source_bindings.len() > 10 {
            return Err(invalid());
        }
        let content: Option<Content> = serde_json::from_str(json).map_err(|_| invalid())?;
        let refs = references(&content)?;
        let input_limit = if matches!(&content, Some(Content::Voice { .. })) {
            2 * MAX_ASSET_BYTES
        } else {
            MAX_ASSET_BYTES
        };
        let input_bytes = inputs
            .iter()
            .try_fold(0usize, |bytes, input| bytes.checked_add(input.bytes.len()))
            .ok_or_else(invalid)?;
        if input_bytes > input_limit {
            return Err(invalid());
        }
        let mut incoming = HashMap::new();
        let mut supplied = Vec::with_capacity(inputs.len());
        for input in inputs {
            if !intent_id(&input.id)
                || input.bytes.is_empty()
                || input.bytes.len() > MAX_ASSET_BYTES
                || !refs.contains(&input.id.as_str())
                || incoming.contains_key(&input.id)
            {
                return Err(invalid());
            }
            let bytes = Zeroizing::new(input.bytes);
            let asset = MediaDraftAsset {
                id: input.id,
                name: input.name,
                mime: input.mime,
                size: bytes.len() as u64,
                sha256: hex::encode(Sha256::digest(&*bytes)),
            };
            if !valid_asset(&asset) {
                return Err(invalid());
            }
            supplied.push(asset.clone());
            incoming.insert(asset.id.clone(), (asset, bytes));
        }
        // The receipt is authoritative for lost native acknowledgements, including
        // cleared/pruned assets. Never infer acceptance from JSON equality alone.
        source_bindings.sort_by(|left, right| left.asset_id.cmp(&right.asset_id));
        if source_bindings
            .windows(2)
            .any(|pair| pair[0].asset_id == pair[1].asset_id)
            || source_bindings.iter().any(|binding| {
                !intent_id(&binding.asset_id)
                    || !binding.reference.is_valid()
                    || !incoming.contains_key(&binding.asset_id)
            })
        {
            return Err(invalid());
        }
        let fingerprint_bytes = if source_bindings.is_empty() {
            serde_json::to_vec(&(
                "zoen.media.draft.save.v1",
                expected,
                content_json(&content)?,
                &supplied,
            ))
        } else {
            serde_json::to_vec(&(
                "zoen.media.draft.save.recent.v1",
                expected,
                content_json(&content)?,
                &supplied,
                &source_bindings,
            ))
        }
        .map_err(|_| invalid())?;
        let fingerprint = hex::encode(Sha256::digest(fingerprint_bytes));
        if let Some(receipt) = previous
            .save_receipts
            .iter()
            .find(|receipt| receipt.input_revision == expected)
        {
            return if receipt.fingerprint == fingerprint {
                Ok(MediaDraftSaveResult {
                    accepted_revision: receipt.accepted_revision.clone().ok_or_else(stale)?,
                    draft: previous.dto()?,
                })
            } else {
                Err(stale())
            };
        }
        if previous.revision != expected || (previous.attempted && content.is_some()) {
            return Err(stale());
        }
        // Replays above prove the original accepted copy. Fresh copies revalidate
        // every source and supplied byte hash before the journal writes anything.
        for binding in &source_bindings {
            let source =
                self.attachment_reference_validate(scope, binding.kind, &binding.reference)?;
            let (asset, _) = incoming.get(&binding.asset_id).ok_or_else(invalid)?;
            if asset.sha256 != source.reference.sha256
                || asset.name != source.name
                || asset.mime != source.mime
                || asset.size != source.bytes
            {
                return Err(invalid());
            }
        }
        let mut assets = Vec::with_capacity(refs.len());
        let mut fresh = Vec::new();
        for id in refs {
            let prior = previous.assets.iter().find(|asset| asset.id == id);
            let asset = if let Some((asset, bytes)) = incoming.remove(id) {
                if let Some(prior) = prior {
                    if prior != &asset || *self.load_media_asset(&storage, prior)? != *bytes {
                        return Err(invalid());
                    }
                } else {
                    fresh.push((asset.clone(), bytes));
                }
                asset
            } else {
                prior.cloned().ok_or_else(invalid)?
            };
            assets.push(asset);
        }
        let revision = types::new_id("media_draft")?;
        let mut save_receipts = previous.save_receipts.clone();
        save_receipts.push(SaveReceipt {
            input_revision: expected.into(),
            fingerprint,
            accepted_revision: Some(revision.clone()),
        });
        if save_receipts.len() > MAX_SAVE_RECEIPTS {
            save_receipts.remove(0);
        }
        let record = Record {
            revision,
            content,
            assets,
            attempted: false,
            last_accepted: previous.last_accepted.clone(),
            save_receipts,
        };
        validate(&record)?;
        self.draft_target(
            &scope.normal(),
            match &record.content {
                Some(Content::Attachments { reply_to, .. } | Content::Voice { reply_to, .. }) => {
                    reply_to.as_deref()
                }
                None => None,
            },
        )?;
        self.atomic_agent_action(|engine| {
            engine.persist_media_replacement(&storage, &previous, &record, &fresh)?;
            Ok(MediaDraftSaveResult {
                accepted_revision: record.revision.clone(),
                draft: record.dto()?,
            })
        })
    }

    pub(super) fn media_draft_asset(
        &self,
        scope: &MediaDraftScope,
        expected: &str,
        asset_id: &str,
    ) -> R<Vec<u8>> {
        let storage = self.media_draft_scope(scope)?;
        let record = self.load_media_draft(&storage)?;
        if !intent_id(expected) || record.revision != expected {
            return Err(stale());
        }
        let asset = record
            .assets
            .iter()
            .find(|asset| asset.id == asset_id)
            .ok_or_else(invalid)?;
        Ok(self.load_media_asset(&storage, asset)?.to_vec())
    }
}
