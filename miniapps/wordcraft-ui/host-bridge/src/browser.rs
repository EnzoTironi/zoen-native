//! Ephemeral adapter for the actual upstream canvas. Core remains the only durable authority.
use super::browser_policy::command_policy;
use crate::{
    Accept, Action, Command, Export, Head, HostBridge, Mutation, Open, Proposal, Result,
    TextReplace, failure, hash, profile,
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use wordcraft_doc::{Document, StoryRef};
use wordcraft_engine::Session;

static NEXT_BINDING: AtomicU64 = AtomicU64::new(1);
struct Entry {
    id: String,
    fingerprint: String,
    proposal: Proposal,
    document: Document,
}
#[derive(Default)]
pub struct BrowserBridge {
    host: Option<HostBridge>,
    binding: Option<u64>,
    document_id: u64,
    accepted: Option<Document>,
    pending: Option<Entry>,
    last: Option<Entry>,
    paused: bool,
}
impl BrowserBridge {
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.host.is_some()
    }
    fn document(&self, s: &Session) -> Result<()> {
        if self.host.is_none()
            || self.binding.is_none()
            || s.managed_binding != self.binding
            || s.document_id() != self.document_id
            || !s.managed_policy.is_some_and(|p| {
                std::ptr::fn_addr_eq(
                    p,
                    command_policy
                        as fn(
                            &Session,
                            &str,
                            &Value,
                        )
                            -> std::result::Result<(), wordcraft_engine::CmdError>,
                )
            })
        {
            return Err(failure("document_changed"));
        }
        Ok(())
    }
    fn host(&self) -> Result<&HostBridge> {
        self.host.as_ref().ok_or_else(|| failure("not_open"))
    }
    fn dirty_unchecked(&self, s: &Session) -> bool {
        self.accepted.as_ref().is_some_and(|doc| doc != &s.doc)
    }
    /// # Errors
    /// Refuses a replaced/unbound upstream Session instead of adopting its content.
    pub fn status(&self, s: &Session) -> Result<Value> {
        self.document(s)?;
        let mut status = self.host()?.status();
        status["dirty"] = json!(self.dirty_unchecked(s));
        status["frozen"] = json!(s.managed_frozen);
        Ok(status)
    }
    /// Open only an exact authorized package. Failed/repeated opens preserve local work and Undo.
    /// # Errors
    /// Refuses changed owner, backward head, another document, pending or unsaved edits.
    pub fn open(&mut self, s: &mut Session, request: Open, bytes: &[u8]) -> Result<Value> {
        if let Some(host) = &self.host {
            self.document(s)?;
            if host.status()["scope"] != json!(request.scope) {
                return Err(failure("scope_changed"));
            }
            if self.pending.is_some() || self.dirty_unchecked(s) {
                return Err(failure("local_changes"));
            }
            let old: Head = serde_json::from_value(host.status()["head"].clone())
                .map_err(|_| failure("invalid_head"))?;
            if request.head.number()? < old.number()?
                || (request.head.version == old.version && request.head != old)
            {
                return Err(failure("stale_head"));
            }
            if request.head == old {
                if hash(bytes) != old.sha256 {
                    return Err(failure("invalid_document_hash"));
                }
                return self.status(s);
            }
        } else if s.managed_binding.is_some() || s.dirty || s.doc != Document::new() {
            return Err(failure("unbound_session"));
        }
        let host = HostBridge::open(request.clone(), bytes)?;
        let doc = profile::parse(bytes)?;
        let binding = NEXT_BINDING
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| failure("document_generation_exhausted"))?;
        let mut next = Session::new(doc.clone());
        next.managed_policy = Some(command_policy);
        next.managed_binding = Some(binding);
        next.managed_frozen = self.paused;
        next.author = request.scope.actor_id;
        next.autosave = false;
        next.prefs.detect_language = false;
        next.prefs.autocorrect.enabled = false;
        next.view.focus_mode = true;
        next.view.ruler = false;
        next.view.mode = wordcraft_engine::layout::ViewMode::Web;
        self.document_id = next.document_id();
        self.binding = Some(binding);
        self.accepted = Some(doc);
        self.host = Some(host);
        self.pending = None;
        self.last = None;
        *s = next;
        self.status(s)
    }
    fn admission(&self, s: &Session, m: &Mutation, fingerprint: &str) -> Result<Option<Proposal>> {
        self.document(s)?;
        if self.host()?.status()["scope"] != json!(m.scope) {
            return Err(failure("scope_changed"));
        }
        m.base.number()?;
        if !crate::wire::identifier(&m.mutation_id) {
            return Err(failure("invalid_mutation"));
        }
        if let Some(last) = self.last.as_ref().filter(|p| p.id == m.mutation_id) {
            return if last.fingerprint == fingerprint {
                Ok(Some(last.proposal.clone()))
            } else {
                Err(failure("mutation_reused"))
            };
        }
        if let Some(pending) = &self.pending {
            if pending.document != s.doc {
                return Err(failure("document_changed"));
            }
            return if pending.id == m.mutation_id && pending.fingerprint == fingerprint {
                Ok(Some(pending.proposal.clone()))
            } else {
                Err(failure("proposal_pending"))
            };
        }
        Ok(None)
    }
    /// Snapshot the actual upstream document. Never turn unsupported rich state into plain text.
    /// # Errors
    /// Refuses stale binding/mutation, unsupported model/writer output, or engine proposal errors.
    pub fn snapshot(&mut self, s: &mut Session, m: Mutation) -> Result<Proposal> {
        let fingerprint = hash(
            &serde_json::to_vec(&json!({"checkpoint":m}))
                .map_err(|_| failure("invalid_request"))?,
        );
        if let Some(proposal) = self.admission(s, &m, &fingerprint)? {
            return Ok(proposal);
        }
        let actual = wordcraft_docx::write(&s.doc).map_err(|_| failure("docx_write"))?;
        let doc = profile::parse(&actual)?;
        let text = doc.plain_text(StoryRef::Body);
        let id = m.mutation_id.clone();
        let mut proposal = self
            .host
            .as_mut()
            .ok_or_else(|| failure("not_open"))?
            .execute(Command {
                mutation: m,
                action: Action::TextReplace(TextReplace { text }),
            })?;
        // Both the model above and the final engine writer are checked. This is an explicit
        // plain-text checkpoint, never a lossy rich-document normalization path.
        proposal.metadata["operation"] = json!({"kind":"humanCheckpoint"});
        self.pending = Some(Entry {
            id,
            fingerprint,
            proposal: proposal.clone(),
            document: s.doc.clone(),
        });
        s.managed_frozen = true;
        Ok(proposal)
    }
    /// Apply the same closed agent command only when the visible manual document is unchanged.
    /// # Errors
    /// Refuses local edits, another owner/document, stale base or incompatible proposals.
    pub fn execute(&mut self, s: &mut Session, command: Command) -> Result<Proposal> {
        let fingerprint =
            hash(&serde_json::to_vec(&command).map_err(|_| failure("invalid_request"))?);
        if let Some(proposal) = self.admission(s, &command.mutation, &fingerprint)? {
            return Ok(proposal);
        }
        if self.dirty_unchecked(s) {
            return Err(failure("local_changes"));
        }
        let id = command.mutation.mutation_id.clone();
        let proposal = self
            .host
            .as_mut()
            .ok_or_else(|| failure("not_open"))?
            .execute(command)?;
        s.managed_frozen = true;
        let doc = profile::parse(&proposal.bytes)?;
        s.set_document(doc);
        self.document_id = s.document_id();
        self.pending = Some(Entry {
            id,
            fingerprint,
            proposal: proposal.clone(),
            document: s.doc.clone(),
        });
        Ok(proposal)
    }
    /// # Errors
    /// Refuses a foreign/late receipt or a changed visible model. ACK cannot undo a host pause.
    pub fn accept(&mut self, s: &mut Session, receipt: &Accept) -> Result<Value> {
        self.document(s)?;
        if let Some(pending) = &self.pending
            && pending.document != s.doc
        {
            return Err(failure("document_changed"));
        }
        self.host
            .as_mut()
            .ok_or_else(|| failure("not_open"))?
            .accept(receipt)?;
        if let Some(mut pending) = self.pending.take() {
            // Preserve the actual canvas Session/Undo history for manual checkpoints.
            self.accepted = Some(pending.document.clone());
            pending.proposal.metadata["acceptedHead"] = json!(receipt.head);
            self.last = Some(pending);
        }
        s.managed_frozen = self.paused;
        self.status(s)
    }
    /// # Errors
    /// Refuses another binding/head. Export always returns exact accepted, never edited bytes.
    pub fn export(&self, s: &Session, request: &Export) -> Result<Proposal> {
        self.document(s)?;
        self.host()?.export(request)
    }
    /// Presentation pause is independent of a pending durable proposal.
    /// # Errors
    /// Refuses a replaced document; caller must first finish any native text composition.
    pub fn freeze(&mut self, s: &mut Session, frozen: bool) -> Result<Value> {
        self.document(s)?;
        self.paused = frozen;
        s.managed_frozen = frozen || self.pending.is_some();
        self.status(s)
    }
}
