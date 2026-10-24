//! Provenance on the existing atomic receipt, never a second execution journal.
use super::*;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceReceipt {
    pub source: String,
    pub source_sha256: String,
    pub request_id: String,
    pub base_version: u32,
    pub base_sha256: String,
}
impl SourceReceipt {
    pub(super) fn valid(&self) -> bool {
        token(&self.source)
            && token(&self.request_id)
            && digest(&self.source_sha256)
            && digest(&self.base_sha256)
            && self.base_version > 0
    }
}
impl Engine {
    /// Every intervening signed version must have been accepted on this device
    /// for this exact source. Missing/evicted provenance fails closed, including
    /// a lateral head authored by the same agent for a different request.
    pub(crate) fn app_binary_scoped_head(
        &self,
        request: &types::ArtifactRequest,
        source: &str,
        source_sha256: &str,
    ) -> R<()> {
        let (me, device, _, current, _) = self.binary_scope(&request.item_id, false)?;
        let key = format!(
            "app_document_receipts:{}",
            hash(&serde_json::to_vec(&(&me, &device, &request.item_id)).map_err(|_| invalid())?)
        );
        let receipts = self.binary_receipts(&key)?;
        let item = self.item_state(&request.item_id)?;
        let mut number = request.version;
        let mut sha = request.sha256.as_str();
        while number < current {
            let next = number.checked_add(1).ok_or_else(stale)?;
            let version = item
                .versions
                .iter()
                .find(|v| v.number == next)
                .ok_or_else(stale)?;
            let accepted = receipts
                .iter()
                .find(|r| {
                    r.item == request.item_id
                        && r.version == next
                        && r.source.as_ref().is_some_and(|s| {
                            s.source == source
                                && s.source_sha256 == source_sha256
                                && s.request_id == request.request_id
                                && s.base_version == number
                                && s.base_sha256 == sha
                        })
                })
                .ok_or_else(stale)?;
            let ItemContent::App(app) = &version.content else {
                return Err(stale());
            };
            if version.author != me
                || app
                    .document
                    .as_ref()
                    .is_none_or(|f| f.sha256 != accepted.sha256)
            {
                return Err(stale());
            }
            number = next;
            sha = &accepted.sha256;
        }
        if number != current {
            return Err(stale());
        }
        Ok(())
    }
}
