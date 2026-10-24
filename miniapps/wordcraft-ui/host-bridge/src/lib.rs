//! Headless adapter over the pinned `WordCraft` Session, using the existing binary host ABI.
//! Proposals are inert bytes. Only the canonical host can authorize and persist a new head.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use wordcraft_doc::StoryRef;
use wordcraft_engine::Session;
#[cfg(feature = "browser")]
pub mod browser;
#[cfg(feature = "browser")]
mod browser_policy;
mod wire;
pub use wire::*;
mod archive;
mod export;
pub mod profile;
#[cfg(test)]
mod tests;

pub type Result<T> = std::result::Result<T, String>;
#[must_use]
pub fn failure(code: &str) -> String {
    code.to_owned()
}
#[must_use]
pub fn hash(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for &byte in &Sha256::digest(bytes) {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 15)]));
    }
    encoded
}

struct Pending {
    fingerprint: String,
    command: Command,
    proposal: Proposal,
}

pub struct HostBridge {
    scope: Scope,
    head: Head,
    accepted_bytes: Vec<u8>,
    session: Session,
    pending: Option<Pending>,
    last: Option<Pending>,
}

impl HostBridge {
    /// Open exact supplied bytes; the caller retains canonical authorization.
    /// # Errors
    /// Refuses malformed scope/head, hash mismatch or an unsupported DOCX profile.
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
        let mut session = Session::new(profile::parse(bytes)?);
        session.author.clone_from(&request.scope.actor_id);
        Ok(Self {
            scope: request.scope,
            head: request.head,
            accepted_bytes: bytes.to_vec(),
            session,
            pending: None,
            last: None,
        })
    }

    #[must_use]
    pub fn status(&self) -> Value {
        json!({"engineRevision":profile::ENGINE_REVISION,"scope":self.scope,"head":self.head,
            "pendingMutationId":self.pending.as_ref().map(|p|&p.command.mutation.mutation_id),
            "profile":"plain-text-v1"})
    }

    #[must_use]
    pub fn inspect(&self) -> Value {
        let mut value = self.status();
        // Inspect the accepted document, even while a proposal is awaiting canonical ACK.
        value["text"] = json!(self.session.doc.plain_text(StoryRef::Body));
        value
    }

    fn scope(&self, scope: &Scope) -> Result<()> {
        if *scope != self.scope {
            return Err(failure("scope_changed"));
        }
        Ok(())
    }

    /// Generate an inert proposal through the actual engine commands.
    /// # Errors
    /// Refuses stale scope/head/intent, pending conflicts, invalid text or engine failure.
    pub fn execute(&mut self, command: Command) -> Result<Proposal> {
        let mutation = &command.mutation;
        self.scope(&mutation.scope)?;
        if !wire::identifier(&mutation.mutation_id) || mutation.base.number()? == u64::MAX {
            return Err(failure("invalid_mutation"));
        }
        let fingerprint =
            hash(&serde_json::to_vec(&command).map_err(|_| failure("invalid_request"))?);
        if let Some(last) = self
            .last
            .as_ref()
            .filter(|p| p.command.mutation.mutation_id == mutation.mutation_id)
        {
            return if last.fingerprint == fingerprint {
                Ok(last.proposal.clone())
            } else {
                Err(failure("mutation_reused"))
            };
        }
        if let Some(pending) = &self.pending {
            return if pending.fingerprint == fingerprint {
                Ok(pending.proposal.clone())
            } else {
                Err(failure("proposal_pending"))
            };
        }
        if mutation.base != self.head {
            return Err(failure("stale_head"));
        }
        let Action::TextReplace(params) = &command.action;
        profile::validate_text(&params.text)?;
        // This worker exposes NO Session/UI/file command dispatcher. Both human and agent
        // callers use this closed operation, translated into the genuine engine commands.
        // Work on an isolated Session so errors/proposals cannot mutate the accepted state.
        let mut next = Session::new(self.session.doc.clone());
        next.author.clone_from(&self.scope.actor_id);
        next.run("select.all", &json!({"whole":true}))
            .map_err(|_| failure("engine_selection"))?;
        next.run("text.insert", &json!({"text":params.text,"raw":true}))
            .map_err(|_| failure("engine_edit"))?;
        let text = next.doc.plain_text(StoryRef::Body);
        if text != params.text {
            return Err(failure("engine_text_mismatch"));
        }
        let bytes = profile::encode(&text)?;
        if profile::parse(&bytes)?.plain_text(StoryRef::Body) != params.text {
            return Err(failure("encoded_text_mismatch"));
        }
        let proposal = Proposal {
            metadata: json!({"scope":mutation.scope,"base":mutation.base,"mutationId":mutation.mutation_id,
                "sha256":hash(&bytes),"byteLength":bytes.len(),"engineRevision":profile::ENGINE_REVISION,
                "operation":{"kind":"command","action":command.action},"acceptedHead":null}),
            bytes,
        };
        self.pending = Some(Pending {
            fingerprint,
            command,
            proposal: proposal.clone(),
        });
        Ok(proposal)
    }

    /// Observe an exact canonical host receipt, never persist or authorize it here.
    /// # Errors
    /// Refuses mismatched scope, proposal, next version or document hash.
    pub fn accept(&mut self, receipt: &Accept) -> Result<Value> {
        self.scope(&receipt.scope)?;
        if self.pending.is_none() {
            return if self.last.as_ref().is_some_and(|p| {
                p.command.mutation.mutation_id == receipt.mutation_id
                    && p.proposal.metadata["acceptedHead"] == json!(receipt.head)
            }) && self.head == receipt.head
            {
                Ok(self.status())
            } else {
                Err(failure("no_proposal"))
            };
        }
        let pending = self
            .pending
            .as_ref()
            .ok_or_else(|| failure("no_proposal"))?;
        let version = self
            .head
            .number()?
            .checked_add(1)
            .ok_or_else(|| failure("version_exhausted"))?;
        if pending.command.mutation.mutation_id != receipt.mutation_id
            || receipt.head.number()? != version
            || receipt.head.sha256 != hash(&pending.proposal.bytes)
        {
            return Err(failure("receipt_mismatch"));
        }
        let doc = profile::parse(&pending.proposal.bytes)?;
        let mut pending = self.pending.take().ok_or_else(|| failure("no_proposal"))?;
        self.session = Session::new(doc);
        self.session.author.clone_from(&self.scope.actor_id);
        self.accepted_bytes.clone_from(&pending.proposal.bytes);
        self.head = receipt.head.clone();
        pending.proposal.metadata["acceptedHead"] = json!(receipt.head);
        self.last = Some(pending);
        Ok(self.status())
    }

    /// Export the exact accepted DOCX or its upstream PDF, never an unacknowledged proposal.
    /// # Errors
    /// Refuses another scope/head, PDF layout/write failure or an oversized export.
    pub fn export(&self, request: &Export) -> Result<Proposal> {
        self.scope(&request.scope)?;
        if request.head != self.head {
            return Err(failure("stale_head"));
        }
        let (bytes, format, mime) = export::accepted(&self.accepted_bytes, &request.format)?;
        Ok(Proposal {
            metadata: json!({"scope":self.scope,"sourceHead":self.head,"format":format,"mime":mime,
                "sha256":hash(&bytes),"byteLength":bytes.len(),
                "engineRevision":profile::ENGINE_REVISION}),
            bytes,
        })
    }
}

#[cfg(all(test, feature = "browser"))]
mod browser_tests;
#[cfg(all(test, feature = "ui-tests"))]
mod browser_ui_tests;

#[cfg(test)]
mod pdf_tests;
