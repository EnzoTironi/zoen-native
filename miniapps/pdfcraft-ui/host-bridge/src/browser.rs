//! A retained actual PdfCraftApp/Session. Canonical bytes and ACKs belong to Core.
use crate::{
    Accept, Command, Export, HostBridge, Mutation, Open, Proposal, Result, failure, hash,
    operation, profile,
};
use pdfcraft_engine::{DocId, Document, Session};
use pdfcraft_ui_egui::PdfCraftApp;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

type WallClock = (fn() -> i64, fn() -> i64);
static NEXT_BINDING: AtomicU64 = AtomicU64::new(1);
enum Kind {
    Manual,
    Agent,
}
struct Entry {
    kind: Kind,
    id: String,
    generation: u64,
}

#[derive(Default)]
pub struct BrowserBridge {
    host: Option<HostBridge>,
    binding: Option<u64>,
    document_id: Option<DocId>,
    accepted_generation: u64,
    pending: Option<Entry>,
    paused: bool,
    wall_clock: Option<WallClock>,
}

impl BrowserBridge {
    /// Uses the host's real clock/time zone for upstream content timestamps.
    #[must_use]
    pub fn with_wall_clock(mut self, clock: fn() -> i64, zone: fn() -> i64) -> Self {
        self.wall_clock = Some((clock, zone));
        self
    }
    /// # Errors
    /// Commits the actual paragraph editor only after its final input frame.
    /// Active composition/contacts and other upstream dialogs/drafts stay pending.
    pub fn finish_input(&self, app: &mut PdfCraftApp, input_active: bool) -> Result<()> {
        self.doc(&app.session)?;
        if input_active {
            return Err(failure("human_interaction"));
        }
        // Reuse upstream's Save/close typing path, including failed-draft restore.
        if !app.managed_finish_input() {
            return Err(failure("paragraph_commit_refused"));
        }
        if app.host_interaction() {
            return Err(failure("human_interaction"));
        }
        Ok(())
    }
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.host.is_some()
    }
    fn host(&self) -> Result<&HostBridge> {
        self.host.as_ref().ok_or_else(|| failure("not_open"))
    }
    fn doc<'a>(&self, session: &'a Session) -> Result<&'a Document> {
        let id = self.document_id.ok_or_else(|| failure("not_open"))?;
        if self.binding.is_none()
            || session.managed_binding != self.binding
            || session.managed_document != Some(id)
            || session.docs().len() != 1
            || session.javascript()
        {
            return Err(failure("document_changed"));
        }
        session.get(id).ok_or_else(|| failure("document_changed"))
    }
    fn dirty(&self, session: &Session) -> Result<bool> {
        Ok(self.doc(session)?.edit_generation() != self.accepted_generation)
    }
    /// # Errors
    /// Refuses a replaced binding/document instead of adopting its bytes.
    pub fn status(&self, session: &Session) -> Result<Value> {
        let mut status = self.host()?.status();
        status["dirty"] = json!(self.dirty(session)?);
        status["generation"] = json!(self.doc(session)?.edit_generation());
        status["frozen"] = json!(session.managed_frozen);
        Ok(status)
    }
    /// # Errors
    /// A newer canonical version may open only over a clean, scoped, idle document.
    pub fn open(&mut self, app: &mut PdfCraftApp, request: Open, bytes: &[u8]) -> Result<Value> {
        if let Some(host) = &self.host {
            self.doc(&app.session)?;
            host.scope(&request.scope)?;
            if self.pending.is_some() || self.dirty(&app.session)? || app.host_interaction() {
                return Err(failure("local_changes"));
            }
            if request.head.number()? < host.head.number()?
                || (request.head.version == host.head.version && request.head != host.head)
            {
                return Err(failure("stale_head"));
            }
            if request.head == host.head {
                if hash(bytes) != host.head.sha256 {
                    return Err(failure("invalid_document_hash"));
                }
                return self.status(&app.session);
            }
        } else if !app.session.docs().is_empty() || app.session.managed_binding.is_some() {
            return Err(failure("unbound_session"));
        }
        let host = HostBridge::open(request, bytes)?;
        if self.host.is_some() {
            return self.adopt_canonical(app, host, bytes);
        }
        let binding = NEXT_BINDING
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| failure("binding_exhausted"))?;
        let mut next = PdfCraftApp::new();
        if let Some((clock, zone)) = self.wall_clock {
            next.session = std::mem::take(&mut next.session).with_wall_clock(clock, zone);
        }
        next.session.set_javascript(false);
        next.open_bytes("Document.pdf", None, bytes.to_vec())
            .map_err(|_| failure("app_open_refused"))?;
        let (_, id) = next.active_ids().ok_or_else(|| failure("not_open"))?;
        next.session.managed_binding = Some(binding);
        next.session.managed_document = Some(id);
        next.session.managed_frozen = self.paused;
        next.managed_export = app.managed_export;
        next.comment_prefs.author.clone_from(&host.scope.actor_id);
        next.set_theme_preference(app.theme_preference);
        next.left_open = false;
        next.set_option("mode", "read")
            .map_err(|_| failure("app_view_refused"))?;
        self.accepted_generation = next
            .session
            .get(id)
            .ok_or_else(|| failure("not_open"))?
            .edit_generation();
        self.document_id = Some(id);
        self.binding = Some(binding);
        self.host = Some(host);
        self.pending = None;
        *app = next;
        self.status(&app.session)
    }
    fn adopt_canonical(
        &mut self,
        app: &mut PdfCraftApp,
        host: HostBridge,
        bytes: &[u8],
    ) -> Result<Value> {
        let current = self.host()?;
        // A metadata-only newer version cannot erase pre-existing native Redo.
        if host.head.sha256 != current.head.sha256 {
            let doc = self.doc(&app.session)?;
            let id = doc.id;
            let generation = doc.edit_generation();
            let frozen = app.session.managed_frozen;
            app.session.managed_frozen = true;
            let adopted = app.session.managed_adopt_canonical(
                id,
                generation,
                std::sync::Arc::new(bytes.to_vec()),
            );
            app.session.managed_frozen = frozen;
            self.accepted_generation =
                adopted.map_err(|_| failure("canonical_adoption_refused"))?;
        }
        self.host = Some(host);
        self.status(&app.session)
    }

    fn admission(
        &self,
        session: &Session,
        mutation: &Mutation,
        fingerprint: &str,
    ) -> Result<Option<Proposal>> {
        let generation = self.doc(session)?.edit_generation();
        if self.pending.as_ref().is_some_and(|entry| {
            matches!(entry.kind, Kind::Agent) && entry.generation != generation
        }) {
            return Err(failure("document_changed"));
        }
        self.host()?.retry(mutation, fingerprint)
    }
    /// # Errors
    /// Retains one exact manual checkpoint without freezing later human edits.
    pub fn snapshot(&mut self, session: &mut Session, mutation: Mutation) -> Result<Proposal> {
        let fingerprint = hash(
            &serde_json::to_vec(&json!({"checkpoint":mutation}))
                .map_err(|_| failure("invalid_request"))?,
        );
        if let Some(proposal) = self.admission(session, &mutation, &fingerprint)? {
            return Ok(proposal);
        }
        let doc = self.doc(session)?;
        if !doc.edit_notices.is_empty() {
            return Err(failure("representation_loss"));
        }
        let generation = doc.edit_generation();
        let id = doc.id;
        let bytes = profile::encode(session, id)?;
        let mutation_id = mutation.mutation_id.clone();
        let proposal = self
            .host
            .as_mut()
            .ok_or_else(|| failure("not_open"))?
            .prepare(
                mutation,
                fingerprint,
                bytes,
                &json!({"kind":"humanCheckpoint"}),
            )?;
        self.pending = Some(Entry {
            kind: Kind::Manual,
            id: mutation_id,
            generation,
        });
        session.managed_frozen = self.paused;
        Ok(proposal)
    }
    /// # Errors
    /// Uses actual engine editing/Undo. A refused result restores its original native editor.
    pub fn execute(&mut self, session: &mut Session, command: Command) -> Result<Proposal> {
        let fingerprint =
            hash(&serde_json::to_vec(&command).map_err(|_| failure("invalid_request"))?);
        if let Some(proposal) = self.admission(session, &command.mutation, &fingerprint)? {
            return Ok(proposal);
        }
        if self.paused || self.dirty(session)? {
            return Err(failure("local_changes"));
        }
        let id = self.doc(session)?.id;
        let checkpoint = session
            .managed_checkpoint(id)
            .map_err(|_| failure("checkpoint_refused"))?;
        let result = (|| {
            operation::apply(
                session,
                id,
                command.action.clone(),
                &command.mutation.scope.actor_id,
            )?;
            let generation = self.doc(session)?.edit_generation();
            let bytes = profile::encode(session, id)?;
            let mutation_id = command.mutation.mutation_id.clone();
            let proposal = self
                .host
                .as_mut()
                .ok_or_else(|| failure("not_open"))?
                .prepare(
                    command.mutation,
                    fingerprint,
                    bytes,
                    &json!({"kind":"command","action":command.action}),
                )?;
            Ok((proposal, mutation_id, generation))
        })();
        let (proposal, mutation_id, generation) = match result {
            Ok(value) => value,
            Err(error) => {
                session
                    .managed_restore(checkpoint)
                    .map_err(|_| failure("rollback_failed"))?;
                return Err(error);
            }
        };
        self.pending = Some(Entry {
            kind: Kind::Agent,
            id: mutation_id,
            generation,
        });
        session.managed_frozen = true;
        Ok(proposal)
    }
    /// # Errors
    /// An older ACK cannot rebase newer native edits OR a newer live UI draft.
    /// A retry is idempotent and only an exact Core receipt advances accepted bytes.
    pub fn accept(
        &mut self,
        session: &mut Session,
        receipt: &Accept,
        interaction: bool,
    ) -> Result<Value> {
        let doc = self.doc(session)?;
        let id = doc.id;
        let generation = doc.edit_generation();
        self.host()?.validate_receipt(receipt)?;
        if let Some(entry) = &self.pending {
            if entry.id != receipt.mutation_id {
                return Err(failure("invalid_receipt"));
            }
            if matches!(entry.kind, Kind::Agent) && entry.generation != generation {
                return Err(failure("document_changed"));
            }
            let stable = entry.generation == generation
                && (!interaction || matches!(entry.kind, Kind::Agent));
            let mut accepted_generation = entry.generation;
            if stable {
                let bytes = self
                    .host()?
                    .pending
                    .as_ref()
                    .ok_or_else(|| failure("no_proposal"))?
                    .proposal
                    .bytes
                    .clone();
                let paused = session.managed_frozen;
                // Exact receipt validation above authorizes this synchronous rebase.
                // No raw UI event can enter during the Rust mutable borrow.
                session.managed_frozen = false;
                let rebased = session.mark_saved(id, std::sync::Arc::new(bytes), None);
                session.managed_frozen = paused;
                rebased.map_err(|_| failure("accepted_rebase_failed"))?;
                accepted_generation = self.doc(session)?.edit_generation();
            }
            self.host
                .as_mut()
                .ok_or_else(|| failure("not_open"))?
                .accept(receipt)?;
            self.accepted_generation = accepted_generation;
            self.pending = None;
        } else {
            self.host
                .as_mut()
                .ok_or_else(|| failure("not_open"))?
                .accept(receipt)?;
        }
        session.managed_frozen = self.paused;
        self.status(session)
    }
    /// # Errors
    /// Exports the selected accepted bytes; pending native/UI edits are never substituted.
    pub fn export(&self, session: &Session, request: &Export) -> Result<Proposal> {
        self.doc(session)?;
        self.host()?.export(request)
    }
    /// # Errors
    /// Caller additionally fences ongoing UI drafts/gestures before suspension.
    pub fn freeze(&mut self, session: &mut Session, frozen: bool) -> Result<Value> {
        self.doc(session)?;
        self.paused = frozen;
        session.managed_frozen = frozen
            || self
                .pending
                .as_ref()
                .is_some_and(|entry| matches!(entry.kind, Kind::Agent));
        self.status(session)
    }
}
