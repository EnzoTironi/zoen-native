//! Existing creation IDs and durable receipt lookup, shared by native and agent paths.
use super::*;

pub(crate) fn creation_item_id(me: &str, device: &str, space: &str, mutation: &str) -> R<String> {
    Ok(format!(
        "it_deck_{}",
        hash(&serde_json::to_vec(&(me, device, space, mutation)).map_err(|_| invalid())?)
    ))
}
impl Engine {
    /// Read only, using the existing durable receipt. Absence never authorizes a
    /// retry: creation still has a deterministic item ID and refuses collisions.
    pub(crate) fn app_binary_creation_result(
        &self,
        space: &str,
        title: &str,
        seed_sha: &str,
        mutation: &str,
    ) -> R<Option<AppBinaryDocumentDto>> {
        let (me, device) = self.app_document_identity(space)?;
        let owner =
            serde_json::to_vec(&(&me, &device, space, deckcraft::APP)).map_err(|_| invalid())?;
        let key = format!("app_document_create_receipts:{}", hash(&owner));
        let fingerprint = hash(
            &serde_json::to_vec(&(
                &me,
                &device,
                space,
                title,
                seed_sha,
                deckcraft::ENGINE_REVISION,
            ))
            .map_err(|_| invalid())?,
        );
        let receipts = self.binary_receipts(&key)?;
        let Some(receipt) = receipts.iter().find(|receipt| receipt.id == mutation) else {
            return Ok(None);
        };
        if receipt.fingerprint != fingerprint
            || receipt.version != 1
            || receipt.sha256 != seed_sha
            || receipt.item != creation_item_id(&me, &device, space, mutation)?
        {
            return Err(storage());
        }
        let accepted =
            self.app_binary_read(&receipt.item, Some(receipt.version), Some(&receipt.sha256))?;
        if self.item_state(&receipt.item)?.space != space || accepted.author_id != me {
            return Err(storage());
        }
        Ok(Some(accepted))
    }
}
