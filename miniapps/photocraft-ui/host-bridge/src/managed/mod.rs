//! Ephemeral scope/head/proposal ABI shared with the managed binary editors.
//! Only Core can authorize a source or persist the inert bytes returned here.
mod wire;
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
pub(crate) fn checked(bytes: &[u8]) -> Result<photocraft_doc::Document> {
    crate::open(bytes).map_err(|_| failure("unsupported_document"))
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
        json!({"engineRevision":"9b9643764f3fb77d9f0e48d7510983ae50621e11", "scope":self.scope,
            "head":self.head,"profile":"raster-canvas-v1",
            "pendingMutationId":self.pending.as_ref().map(|p|&p.mutation.mutation_id)})
    }
    /// # Errors
    /// Reads dimensions from the actual accepted document, never from pending pixels.
    pub fn inspect(&self) -> Result<Value> {
        let metadata = crate::inspect(&self.accepted, &Sha256::digest(&self.accepted).into())
            .map_err(|_| failure("accepted_document_refused"))?;
        let mut value = self.status();
        value["width"] = json!(metadata.width);
        value["height"] = json!(metadata.height);
        value["layers"] = json!(metadata.layers);
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
            "engineRevision":"9b9643764f3fb77d9f0e48d7510983ae50621e11","operation":operation}),
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
        let bytes = crate::transform(
            &self.accepted,
            &Sha256::digest(&self.accepted).into(),
            command.action,
        )
        .map_err(|_| failure("engine_command_refused"))?;
        self.prepare(command.mutation, fingerprint, bytes, &json!(command.action))
    }
    /// # Errors
    /// Only the exact retained proposal at its next version can advance accepted bytes.
    pub fn accept(&mut self, receipt: &Accept) -> Result<Value> {
        self.scope(&receipt.scope)?;
        if self.pending.is_none()
            && self.last.as_ref().is_some_and(|p| {
                p.mutation.mutation_id == receipt.mutation_id
                    && receipt.head == self.head
                    && p.proposal.metadata["sha256"] == receipt.head.sha256
            })
        {
            return Ok(self.status());
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
            ExportFormat::Pcraft => (self.accepted.clone(), "application/x-photocraft"),
            ExportFormat::Png => (
                crate::export_png(&self.accepted, &Sha256::digest(&self.accepted).into())
                    .map_err(|_| failure("export_refused"))?,
                "image/png",
            ),
        };
        Ok(Proposal {
            metadata: json!({"scope":self.scope,"head":self.head,"mime":mime,
            "sha256":hash(&bytes),"byteLength":bytes.len()}),
            bytes,
        })
    }
}
