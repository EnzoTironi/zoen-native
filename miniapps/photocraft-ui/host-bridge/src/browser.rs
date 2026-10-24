//! Actual upstream Session and Undo; only the host owns canonical persistence.
use crate::managed::{
    self, Accept, Command, Export, Head, HostBridge, Mutation, Open, Proposal, Result, failure,
    hash,
};
use photocraft_doc::Document;
use photocraft_engine::{EngineError, Session};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

static NEXT_BINDING: AtomicU64 = AtomicU64::new(1);
enum Kind {
    Manual,
    Agent,
}
struct Entry {
    kind: Kind,
    id: String,
    document: Arc<Document>,
}
#[derive(Default)]
pub struct BrowserBridge {
    host: Option<HostBridge>,
    binding: Option<u64>,
    document_id: u64,
    accepted: Option<Arc<Document>>,
    pending: Option<Entry>,
    paused: bool,
}
fn denied() -> EngineError {
    EngineError::Other("This action is unavailable in this document.".into())
}
/// # Errors
/// Closed raster commands only; no file, plugin, network, font or account commands.
pub fn command_policy(s: &Session, id: &str, _params: &Value) -> photocraft_engine::Result<()> {
    if s.managed_frozen {
        return Err(denied());
    }
    match id {
        "image.imageRotation.flipCanvasHorizontal"
        | "image.imageRotation.flipCanvasVertical"
        | "image.imageRotation.90cw"
        | "image.imageRotation.90ccw"
        | "image.imageRotation.180"
        | "paint.stroke"
        | "paint.pencil"
        | "tools.setColors"
        | "edit.undo"
        | "edit.redo"
        | "view.zoomIn"
        | "view.zoomOut"
        | "view.fitOnScreen"
        | "view.100"
        | "view.zoom" => Ok(()),
        _ => Err(denied()),
    }
}
impl BrowserBridge {
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.host.is_some()
    }
    fn doc<'a>(&self, s: &'a Session) -> Result<&'a Arc<Document>> {
        if self.binding.is_none()
            || s.managed_binding != self.binding
            || s.documents().len() != 1
            || !s.managed_policy.is_some_and(|p| {
                std::ptr::fn_addr_eq(
                    p,
                    command_policy as fn(&Session, &str, &Value) -> photocraft_engine::Result<()>,
                )
            })
        {
            return Err(failure("document_changed"));
        }
        let doc = &s.active().ok_or_else(|| failure("not_open"))?.doc;
        if doc.id.0 != self.document_id {
            return Err(failure("document_changed"));
        }
        Ok(doc)
    }
    fn host(&self) -> Result<&HostBridge> {
        self.host.as_ref().ok_or_else(|| failure("not_open"))
    }
    fn dirty(&self, s: &Session) -> Result<bool> {
        Ok(self.accepted.as_ref() != Some(self.doc(s)?))
    }
    /// # Errors
    /// Refuses a replaced binding/document rather than adopting its pixels.
    pub fn status(&self, s: &Session) -> Result<Value> {
        let mut status = self.host()?.status();
        status["dirty"] = json!(self.dirty(s)?);
        status["frozen"] = json!(s.managed_frozen);
        Ok(status)
    }
    /// # Errors
    /// Requires clean exact supplied bytes; opening cannot discard local work/Undo.
    pub fn open(&mut self, s: &mut Session, request: Open, bytes: &[u8]) -> Result<Value> {
        if let Some(host) = &self.host {
            self.doc(s)?;
            if host.status()["scope"] != json!(request.scope) {
                return Err(failure("scope_changed"));
            }
            if self.pending.is_some() || self.dirty(s)? {
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
        } else if !s.documents().is_empty() || s.managed_binding.is_some() {
            return Err(failure("unbound_session"));
        }
        let host = HostBridge::open(request, bytes)?;
        let doc = Arc::new(managed::checked(bytes)?);
        let binding = NEXT_BINDING
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| failure("binding_exhausted"))?;
        let mut next = Session::new();
        next.add_document((*doc).clone(), None);
        next.managed_policy = Some(command_policy);
        next.managed_binding = Some(binding);
        next.managed_frozen = self.paused;
        self.document_id = doc.id.0;
        self.binding = Some(binding);
        self.accepted = Some(doc);
        self.host = Some(host);
        self.pending = None;
        *s = next;
        self.status(s)
    }
    fn admission(&self, s: &Session, m: &Mutation, fingerprint: &str) -> Result<Option<Proposal>> {
        let doc = self.doc(s)?;
        if let Some(p) = &self.pending
            && matches!(p.kind, Kind::Agent)
            && p.document != *doc
        {
            return Err(failure("document_changed"));
        }
        self.host()?.retry(m, fingerprint)
    }
    /// # Errors
    /// Retains one immutable original checkpoint; later manual strokes keep their real Undo.
    pub fn snapshot(&mut self, s: &mut Session, m: Mutation) -> Result<Proposal> {
        let fingerprint = hash(
            &serde_json::to_vec(&json!({"checkpoint":m}))
                .map_err(|_| failure("invalid_request"))?,
        );
        if let Some(p) = self.admission(s, &m, &fingerprint)? {
            return Ok(p);
        }
        if self.paused {
            return Err(failure("host_paused"));
        }
        let doc = self.doc(s)?.clone();
        let bytes =
            photocraft_format::save_to_bytes(&doc, &photocraft_format::SaveOptions::default())
                .map_err(|_| failure("document_encoding"))?;
        let id = m.mutation_id.clone();
        let proposal = self
            .host
            .as_mut()
            .ok_or_else(|| failure("not_open"))?
            .prepare(m, fingerprint, bytes, &json!({"kind":"humanCheckpoint"}))?;
        self.pending = Some(Entry {
            kind: Kind::Manual,
            id,
            document: doc,
        });
        // Ordinary autosave must not freeze a continuing human brush stroke.
        s.managed_frozen = self.paused;
        Ok(proposal)
    }
    /// # Errors
    /// Runs the closed command in the actual visible Session, only on its accepted baseline.
    pub fn execute(&mut self, s: &mut Session, command: Command) -> Result<Proposal> {
        let fingerprint =
            hash(&serde_json::to_vec(&command).map_err(|_| failure("invalid_request"))?);
        if let Some(p) = self.admission(s, &command.mutation, &fingerprint)? {
            return Ok(p);
        }
        if self.paused || self.dirty(s)? {
            return Err(failure("local_changes"));
        }
        s.execute(command.action.command(), json!({}))
            .map_err(|_| failure("engine_command_refused"))?;
        let doc = self.doc(s)?.clone();
        let bytes =
            photocraft_format::save_to_bytes(&doc, &photocraft_format::SaveOptions::default())
                .map_err(|_| failure("document_encoding"))?;
        let id = command.mutation.mutation_id.clone();
        let proposal = self
            .host
            .as_mut()
            .ok_or_else(|| failure("not_open"))?
            .prepare(command.mutation, fingerprint, bytes, &json!(command.action))?;
        self.pending = Some(Entry {
            kind: Kind::Agent,
            id,
            document: doc,
        });
        s.managed_frozen = true;
        Ok(proposal)
    }
    /// # Errors
    /// Exact receipt advances only the baseline; no older ACK overwrites newer manual input.
    pub fn accept(&mut self, s: &mut Session, receipt: &Accept) -> Result<Value> {
        self.doc(s)?;
        if let Some(p) = &self.pending {
            if p.id != receipt.mutation_id {
                return Err(failure("invalid_receipt"));
            }
            if matches!(p.kind, Kind::Agent) && &p.document != self.doc(s)? {
                return Err(failure("document_changed"));
            }
        }
        self.host
            .as_mut()
            .ok_or_else(|| failure("not_open"))?
            .accept(receipt)?;
        if let Some(p) = self.pending.take() {
            self.accepted = Some(p.document);
        }
        s.managed_frozen = self.paused;
        self.status(s)
    }
    /// # Errors
    /// Export accepted bytes only, independent from newer manual state/Undo.
    pub fn export(&self, s: &Session, request: &Export) -> Result<Proposal> {
        self.doc(s)?;
        self.host()?.export(request)
    }
    /// # Errors
    /// Refuses a replaced document. The consumer also fences in-flight gestures before pause.
    pub fn freeze(&mut self, s: &mut Session, frozen: bool) -> Result<Value> {
        self.doc(s)?;
        self.paused = frozen;
        s.managed_frozen = frozen
            || self
                .pending
                .as_ref()
                .is_some_and(|p| matches!(p.kind, Kind::Agent));
        self.status(s)
    }
}
