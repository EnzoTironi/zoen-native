//! Ephemeral scope/head/proposal ABI shared with the managed binary editors.
//! Only Core can authorize a source or persist the inert bytes returned here.
#[cfg(feature = "browser")]
pub mod browser;
pub mod operation;
pub mod profile;
mod structure;
#[cfg(test)]
mod tests;
mod wire;
pub use operation::Operation;
pub const MAX_DOCUMENT_BYTES: usize = structure::MAX_BYTES;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub use wire::*;

pub type Result<T> = std::result::Result<T, String>;
#[must_use]
pub fn failure(code: &str) -> String {
    code.to_owned()
}
#[must_use]
pub fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut out, b| {
            const HEX: &[u8; 16] = b"0123456789abcdef";
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 15)]));
            out
        })
}
pub(crate) fn checked(bytes: &[u8]) -> Result<()> {
    profile::checked(bytes)
}
struct Pending {
    fingerprint: String,
    mutation: Mutation,
    proposal: Proposal,
}
pub struct HostBridge {
    scope: Scope,
    head: Head,
    accepted: Vec<u8>,
    pending: Option<Pending>,
    last: Option<Pending>,
}
impl HostBridge {
    /// # Errors
    /// Refuses an invalid scope/head/hash or a document outside the lossless profile.
    pub fn open(request: Open, bytes: &[u8]) -> Result<Self> {
        if !wire::identifier(&request.scope.artifact_id)
            || !wire::identifier(&request.scope.actor_id)
        {
            return Err(failure("invalid_scope"));
        }
        request.head.number()?;
        if hash(bytes) != request.head.sha256 {
            return Err(failure("invalid_document_hash"));
        }
        checked(bytes)?;
        Ok(Self {
            scope: request.scope,
            head: request.head,
            accepted: bytes.to_vec(),
            pending: None,
            last: None,
        })
    }
    #[must_use]
    pub fn status(&self) -> Value {
        json!({"engineRevision":"d445410807e7eefbe5b96f9662ab963ae8dd2a3a", "scope":self.scope,
            "head":self.head,"profile":"review-pdf-v1",
            "pendingMutationId":self.pending.as_ref().map(|p|&p.mutation.mutation_id)})
    }
    /// # Errors
    /// Reads dimensions from the actual accepted document, never from pending pixels.
    pub fn inspect(&self) -> Result<Value> {
        self.inspect_query(None)
    }
    /// # Errors
    /// Reads a bounded page/paragraph window from the actual accepted document only.
    pub fn inspect_query(&self, query: Option<&ReadQuery>) -> Result<Value> {
        let (session, id) = profile::open(&self.accepted)?;
        let document = session.get(id).ok_or_else(|| failure("not_open"))?;
        let (page, offset) = query.map_or((0, 0), |query| (query.page, query.offset));
        if page >= document.info.pages.len() || offset > 8192 {
            return Err(failure("invalid_read"));
        }
        let paragraphs = document.text_blocks(page);
        let blocks = paragraphs.iter().enumerate().skip(offset).take(16).map(|(index, block)| {
            json!({"index":index,"text":block.text.chars().take(512).collect::<String>(),"rect":block.rect,
                "complete":block.text.chars().count()<=512})
        }).collect::<Vec<_>>();
        let next = (offset.saturating_add(blocks.len()) < paragraphs.len())
            .then_some(offset.saturating_add(blocks.len()));
        let mut value = self.status();
        value["pageCount"] = json!(document.info.pages.len());
        value["pages"] = json!(document.info.pages.iter().enumerate().map(|(index, page)|
            json!({"index":index,"width":page.width,"height":page.height,"rotation":page.rotation})).collect::<Vec<_>>());
        value["page"] = json!(page);
        value["blocks"] = json!(blocks);
        value["nextOffset"] = json!(next);
        if let Some(query) = query {
            value["query"] = json!(query);
        }
        Ok(value)
    }
    fn scope(&self, scope: &Scope) -> Result<()> {
        if *scope != self.scope {
            return Err(failure("scope_changed"));
        }
        Ok(())
    }
    pub(crate) fn retry(&self, m: &Mutation, fingerprint: &str) -> Result<Option<Proposal>> {
        self.scope(&m.scope)?;
        if !wire::identifier(&m.mutation_id) || m.base.number()? == u64::MAX {
            return Err(failure("invalid_mutation"));
        }
        if let Some(last) = self
            .last
            .as_ref()
            .filter(|p| p.mutation.mutation_id == m.mutation_id)
        {
            return if last.fingerprint == fingerprint {
                Ok(Some(last.proposal.clone()))
            } else {
                Err(failure("mutation_reused"))
            };
        }
        if let Some(p) = &self.pending {
            return if p.mutation.mutation_id == m.mutation_id && p.fingerprint == fingerprint {
                Ok(Some(p.proposal.clone()))
            } else {
                Err(failure("proposal_pending"))
            };
        }
        if m.base != self.head {
            return Err(failure("stale_head"));
        }
        Ok(None)
    }
    pub(crate) fn prepare(
        &mut self,
        m: Mutation,
        fingerprint: String,
        bytes: Vec<u8>,
        operation: &Value,
    ) -> Result<Proposal> {
        if let Some(p) = self.retry(&m, &fingerprint)? {
            return Ok(p);
        }
        checked(&bytes)?;
        let proposal = Proposal {
            metadata: json!({"scope":self.scope,"base":self.head,
            "mutationId":m.mutation_id,"sha256":hash(&bytes),"byteLength":bytes.len(),
            "engineRevision":"d445410807e7eefbe5b96f9662ab963ae8dd2a3a","operation":operation}),
            bytes,
        };
        self.pending = Some(Pending {
            fingerprint,
            mutation: m,
            proposal: proposal.clone(),
        });
        Ok(proposal)
    }
    /// # Errors
    /// Refuses stale owner/base/intent or an incompatible pending proposal.
    pub fn execute(&mut self, command: Command) -> Result<Proposal> {
        let fingerprint =
            hash(&serde_json::to_vec(&command).map_err(|_| failure("invalid_request"))?);
        if let Some(p) = self.retry(&command.mutation, &fingerprint)? {
            return Ok(p);
        }
        let (mut session, id) = profile::open(&self.accepted)?;
        operation::apply(
            &mut session,
            id,
            command.action.clone(),
            &self.scope.actor_id,
        )?;
        let bytes = profile::encode(&session, id)?;
        self.prepare(
            command.mutation,
            fingerprint,
            bytes,
            &json!({"kind":"command","action":command.action}),
        )
    }
    /// # Errors
    /// Only the exact retained proposal at its next version can advance accepted bytes.
    pub(crate) fn validate_receipt(&self, receipt: &Accept) -> Result<()> {
        self.scope(&receipt.scope)?;
        if self.pending.is_none()
            && self.last.as_ref().is_some_and(|p| {
                p.mutation.mutation_id == receipt.mutation_id
                    && receipt.head == self.head
                    && p.proposal.metadata["sha256"] == receipt.head.sha256
            })
        {
            return Ok(());
        }
        let p = self
            .pending
            .as_ref()
            .ok_or_else(|| failure("no_proposal"))?;
        if p.mutation.mutation_id != receipt.mutation_id
            || receipt.head.sha256 != hash(&p.proposal.bytes)
            || receipt.head.number()?
                != self
                    .head
                    .number()?
                    .checked_add(1)
                    .ok_or_else(|| failure("version_exhausted"))?
        {
            return Err(failure("invalid_receipt"));
        }
        Ok(())
    }
    /// # Errors
    /// Advances accepted bytes only for the exact validated retained proposal.
    pub fn accept(&mut self, receipt: &Accept) -> Result<Value> {
        self.validate_receipt(receipt)?;
        if self.pending.is_none() {
            return Ok(self.status());
        }
        let p = self.pending.take().ok_or_else(|| failure("no_proposal"))?;
        self.accepted.clone_from(&p.proposal.bytes);
        self.head = receipt.head.clone();
        self.last = Some(p);
        Ok(self.status())
    }
    /// # Errors
    /// Export only the exact accepted version, never the pending/live canvas buffer.
    pub fn export(&self, request: &Export) -> Result<Proposal> {
        self.scope(&request.scope)?;
        if request.head != self.head {
            return Err(failure("stale_head"));
        }
        let (bytes, mime) = match request.format {
            ExportFormat::Pdf => (self.accepted.clone(), "application/pdf"),
        };
        Ok(Proposal {
            metadata: json!({"scope":self.scope,"sourceHead":self.head,"mime":mime,
            "format":request.format,"engineRevision":"d445410807e7eefbe5b96f9662ab963ae8dd2a3a",
            "sha256":hash(&bytes),"byteLength":bytes.len()}),
            bytes,
        })
    }
}
