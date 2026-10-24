//! Managed adapter over the actual pinned GridCraft Session. No host storage.
use gridcraft_engine::{DocState, Session, io};
use gridcraft_model::Workbook;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Arc;
mod wire;
pub use wire::*;
// Build preparation copies the SINGLE canonical miniapps profile, without edits.
mod policy;
pub mod profile;
pub use policy::managed_command_policy;
#[cfg(test)]
mod tests;
#[cfg(all(test, feature = "ui-tests"))]
mod ui_tests;
pub type Result<T> = std::result::Result<T, String>;
pub fn failure(s: &str) -> String {
    s.to_owned()
}
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn engine(e: gridcraft_engine::EngineError) -> String {
    format!("engine: {e}")
}
struct Pending {
    id: String,
    fingerprint: String,
    doc: Arc<Workbook>,
    proposal: Proposal,
}
struct Accepted {
    id: String,
    fingerprint: String,
    proposal: Proposal,
}
#[derive(Default)]
pub struct HostBridge {
    scope: Option<Scope>,
    head: Option<Head>,
    uid: Option<u64>,
    accepted: Option<Arc<Workbook>>,
    accepted_bytes: Vec<u8>,
    pending: Option<Pending>,
    last: Option<Accepted>,
    paused: bool,
}
impl HostBridge {
    pub fn is_open(&self) -> bool {
        self.scope.is_some() && self.head.is_some() && self.uid.is_some()
    }
    fn document(&self, s: &Session) -> Result<()> {
        if s.documents().len() != 1 || s.active().map(|d| d.uid) != self.uid || self.uid.is_none() {
            return Err(failure("document_changed"));
        }
        Ok(())
    }
    fn scope(&self, scope: &Scope) -> Result<()> {
        if self.scope.as_ref() != Some(scope) {
            Err(failure("scope_changed"))
        } else {
            Ok(())
        }
    }
    pub fn dirty(&self, s: &Session) -> bool {
        match (self.accepted.as_ref(), s.active()) {
            (Some(a), Some(d)) => !Arc::ptr_eq(a, &d.wb),
            _ => false,
        }
    }
    pub fn status(&self, s: &Session) -> Value {
        json!({"engineRevision":profile::ENGINE_REVISION,"scope":self.scope,"head":self.head,"dirty":self.dirty(s),"frozen":s.managed_frozen,"pendingMutationId":self.pending.as_ref().map(|p|&p.id)})
    }
    /// Presentation pause is independent of an unresolved durable proposal.
    pub fn freeze(&mut self, s: &mut Session, frozen: bool) -> Result<Value> {
        self.document(s)?;
        if frozen && !matches!(s.mode, gridcraft_engine::Mode::Ready) {
            return Err(failure("human_interaction"));
        }
        self.paused = frozen;
        s.managed_frozen = frozen || self.pending.is_some();
        Ok(self.status(s))
    }
    pub fn open(&mut self, s: &mut Session, request: Open, bytes: &[u8]) -> Result<Value> {
        if !wire::identifier(&request.scope.artifact_id)
            || !wire::identifier(&request.scope.actor_id)
        {
            return Err(failure("invalid_scope"));
        }
        request.head.number()?;
        if !self.is_open() && !s.documents().is_empty() {
            return Err(failure("unbound_session"));
        }
        if !matches!(s.mode, gridcraft_engine::Mode::Ready) {
            return Err(failure("human_interaction"));
        }
        if let Some(scope) = &self.scope {
            self.scope(&request.scope)?;
            self.document(s)?;
            if scope != &request.scope || self.pending.is_some() || self.dirty(s) {
                return Err(failure("local_changes"));
            }
            let old = self.head.as_ref().ok_or_else(|| failure("not_open"))?;
            if request.head.number()? < old.number()?
                || (request.head.version == old.version && request.head != *old)
            {
                return Err(failure("stale_head"));
            }
        }
        profile::Document::parse(bytes).map_err(failure)?;
        if hash(bytes) != request.head.sha256 {
            return Err(failure("invalid_document_hash"));
        }
        let (wb, warnings) = io::open_bytes("Workbook.xlsx", bytes).map_err(engine)?;
        if !warnings.is_empty() {
            return Err(failure("import_requires_review"));
        }
        let mut next = Session::new();
        next.prefs.user_name = request.scope.actor_id.clone();
        next.add_document(DocState::new(wb, None, "Workbook".into()));
        next.command_policy = Some(managed_command_policy);
        next.managed_frozen = self.paused;
        self.uid = next.active().map(|d| d.uid);
        self.accepted = next.active().map(|d| d.wb.clone());
        self.accepted_bytes = bytes.to_vec();
        self.scope = Some(request.scope);
        self.head = Some(request.head);
        self.pending = None;
        self.last = None;
        *s = next;
        Ok(self.status(s))
    }
    fn admission(&self, s: &Session, m: &Mutation, fingerprint: &str) -> Result<Option<Proposal>> {
        self.scope(&m.scope)?;
        self.document(s)?;
        if !wire::identifier(&m.mutation_id) || m.base.number()? == u64::MAX {
            return Err(failure("invalid_mutation"));
        }
        if let Some(last) = self.last.as_ref().filter(|last| last.id == m.mutation_id) {
            return if last.fingerprint == fingerprint {
                Ok(Some(last.proposal.clone()))
            } else {
                Err(failure("mutation_reused"))
            };
        }
        if let Some(p) = &self.pending {
            return if p.id == m.mutation_id && p.fingerprint == fingerprint {
                Ok(Some(p.proposal.clone()))
            } else {
                Err(failure("proposal_pending"))
            };
        }
        if self.head.as_ref() != Some(&m.base) {
            return Err(failure("stale_head"));
        }
        if !matches!(s.mode, gridcraft_engine::Mode::Ready) {
            return Err(failure("human_interaction"));
        }
        Ok(None)
    }
    fn prepare(
        &mut self,
        s: &mut Session,
        m: Mutation,
        fingerprint: String,
        operation: Value,
    ) -> Result<Proposal> {
        let doc = s.doc().map_err(engine)?.wb.clone();
        let bytes = io::save_bytes(&doc, "Workbook.xlsx").map_err(engine)?;
        profile::Document::parse(&bytes).map_err(failure)?;
        let proposal = Proposal {
            metadata: json!({"scope":m.scope,"base":m.base,"mutationId":m.mutation_id,"sha256":hash(&bytes),"byteLength":bytes.len(),"engineRevision":profile::ENGINE_REVISION,"operation":operation,"acceptedHead":null}),
            bytes,
        };
        self.pending = Some(Pending {
            id: m.mutation_id,
            fingerprint,
            doc,
            proposal: proposal.clone(),
        });
        s.managed_frozen = true;
        Ok(proposal)
    }
    pub fn snapshot(&mut self, s: &mut Session, m: Mutation) -> Result<Proposal> {
        let fingerprint = hash(
            &serde_json::to_vec(&json!({"checkpoint":m}))
                .map_err(|_| failure("invalid_request"))?,
        );
        if let Some(p) = self.admission(s, &m, &fingerprint)? {
            return Ok(p);
        }
        self.prepare(s, m, fingerprint, json!({"kind":"humanCheckpoint"}))
    }
    pub fn execute(&mut self, s: &mut Session, c: Command) -> Result<Proposal> {
        let fingerprint = hash(&serde_json::to_vec(&c).map_err(|_| failure("invalid_request"))?);
        if let Some(p) = self.admission(s, &c.mutation, &fingerprint)? {
            return Ok(p);
        }
        if self.dirty(s) {
            return Err(failure("local_changes"));
        }
        let before = s.doc().map_err(engine)?.clone();
        let journal = s.journal.len();
        let requests = s.ui_requests.len();
        let result = (|| {
            let Action::CellSet(p) = &c.action;
            let value = s
                .execute(
                    "cell.set",
                    json!({"sheet":p.sheet,"cell":p.cell,"input":p.input}),
                )
                .map_err(engine)?;
            self.prepare(
                s,
                c.mutation,
                fingerprint,
                json!({"kind":"command","action":c.action,"result":value}),
            )
        })();
        if result.is_err() {
            if let Some(d) = s.active_mut() {
                *d = before;
            }
            s.journal.truncate(journal);
            s.ui_requests.truncate(requests);
        }
        result
    }
    pub fn accept(&mut self, s: &mut Session, r: Accept) -> Result<Value> {
        self.scope(&r.scope)?;
        self.document(s)?;
        if self.pending.is_none() {
            return if self.last.as_ref().is_some_and(|last| {
                last.id == r.mutation_id && last.proposal.metadata["acceptedHead"] == json!(r.head)
            }) && self.head.as_ref() == Some(&r.head)
            {
                Ok(self.status(s))
            } else {
                Err(failure("no_proposal"))
            };
        }
        let p = self
            .pending
            .as_ref()
            .ok_or_else(|| failure("no_proposal"))?;
        let next = self
            .head
            .as_ref()
            .ok_or_else(|| failure("not_open"))?
            .number()?
            .checked_add(1)
            .ok_or_else(|| failure("version_exhausted"))?;
        if p.id != r.mutation_id
            || r.head.number()? != next
            || hash(&p.proposal.bytes) != r.head.sha256
        {
            return Err(failure("receipt_mismatch"));
        }
        let mut p = self.pending.take().ok_or_else(|| failure("no_proposal"))?;
        self.accepted = Some(p.doc.clone());
        self.accepted_bytes = p.proposal.bytes.clone();
        if let Some(d) = s.active_mut() {
            d.saved = p.doc;
        }
        self.head = Some(r.head.clone());
        p.proposal.metadata["acceptedHead"] = json!(r.head);
        self.last = Some(Accepted {
            id: p.id,
            fingerprint: p.fingerprint,
            proposal: p.proposal,
        });
        s.managed_frozen = self.paused;
        Ok(self.status(s))
    }
    pub fn export(&self, s: &Session, r: Export) -> Result<Proposal> {
        self.scope(&r.scope)?;
        self.document(s)?;
        if self.head.as_ref() != Some(&r.head) {
            return Err(failure("stale_head"));
        }
        // XLSX is the native canonical format. Export the SELECTED accepted bytes,
        // not a newer draft or a lossy reserialization of imported content.
        let bytes = self.accepted_bytes.clone();
        Ok(Proposal {
            metadata: json!({"scope":r.scope,"sourceHead":r.head,"format":"xlsx","mime":profile::MIME,"sha256":hash(&bytes),"byteLength":bytes.len(),"engineRevision":profile::ENGINE_REVISION}),
            bytes,
        })
    }
}
