//! Ordered envelope offers and per-Space publication fences.

use super::*;

impl Engine {
    /// Upload availability is not write authority: signed/canonical admission still
    /// runs normally. This only resolves the File bytes an outbox intent references.
    pub(super) fn outbox_attachment<'a>(&'a self, event: &'a Event) -> Option<&'a FileDoc> {
        let content = match &event.body {
            EventBody::ItemCreated { content, .. } | EventBody::ItemVersioned { content, .. } => {
                content
            }
            EventBody::MessagePosted {
                attaches: Some(item),
                ..
            } => &self.state.items.get(item)?.current().content,
            _ => return None,
        };
        if let ItemContent::File(file) = content {
            Some(file)
        } else {
            None
        }
    }

    pub fn cursors(&self) -> Vec<Cursor> {
        self.net
            .synced
            .iter()
            .map(|s| Cursor {
                space: s.clone(),
                next_seq: self.logs.get(s).map(|l| l.next_seq()).unwrap_or(0),
            })
            .collect()
    }

    pub fn next_seq(&self, space: &str) -> u64 {
        self.logs.get(space).map(|l| l.next_seq()).unwrap_or(0)
    }

    /// Envelopes still waiting for the relay, oldest first.
    pub fn outbox_envelopes(&self) -> Vec<Envelope> {
        self.outbox_envelopes_except(&HashMap::new())
    }

    /// The outbox as envelopes, leaving out the ones already sent on this connection.
    /// Skips sent entries before sealing work and asks each Space's group for its epoch
    /// once. Media admission separately folds confirmed history once per relevant Space
    /// and offer pass; that replay cost is not a constant-time publication guarantee.
    ///
    /// At most [`APPLICATION_IN_FLIGHT`] application messages per Space are unpublished
    /// or still awaiting accept. Once a Space is at that cap, every later head of that
    /// same Space stays queued, in order, until a confirmation frees a slot. Heads of
    /// other Spaces keep publishing, each under the same cap. OpenMLS drops a generation
    /// more than that far behind the one it has already opened (`TooDistantInThePast`),
    /// and a rate-limit reject retries the older ciphertext after a later one has been
    /// sequenced.
    pub fn outbox_envelopes_except(&self, sent: &HashMap<String, Option<u64>>) -> Vec<Envelope> {
        let mut ready = HashMap::new();
        let mut media_dependencies = HashMap::new();
        // A Welcome goes out only once its commit is in: sent together, a commit held back
        // by the relay (rate limit) would let the Welcome arrive first and be refused.
        let held = self.welcomes_behind_commits();
        // Ahead of any in-flight application cap: a forked Space publishes nothing.
        let mut forked = HashMap::new();
        let heads = self.store.outbox_heads().unwrap_or_default();
        let mut in_flight: HashMap<String, usize> = HashMap::new();
        for head in &heads {
            if sent
                .get(&head.client_id)
                .is_some_and(|epoch| epoch.is_some())
            {
                *in_flight.entry(head.space.clone()).or_default() += 1;
            }
        }
        let mut blocked: HashSet<String> = HashSet::new();
        let mut out = Vec::new();
        for p in heads {
            if blocked.contains(&p.space) {
                continue;
            }
            if p.failed || sent.contains_key(&p.client_id) || held.contains(&p.client_id) {
                continue;
            }
            if *forked
                .entry(p.space.clone())
                .or_insert_with(|| self.fork_recorded(&p.space))
            {
                continue;
            }
            // Refused for being clear in a Space that went end-to-end: it waits until this
            // device has caught up with that, then goes out sealed.
            if p.last_error.as_deref() == Some(protocol::SEAL_REQUIRED) && !self.is_e2e(&p.space) {
                continue;
            }
            let Some(event) = self.store.outbox_get(&p.client_id).ok().flatten() else {
                continue;
            };
            // An offline creation has a predicted genesis link. Its descendants
            // must wait for confirmation: a retryable creation failure can otherwise
            // let them reach the relay before the Space exists.
            if event
                .seen
                .as_ref()
                .is_some_and(|seen| self.next_seq(&event.space) <= seen.seq)
            {
                continue;
            }
            // File publication must not outrun its actual HTTP upload. A terminal
            // refusal retains this row and fails the exact dependent outbox intents;
            // transient retries keep the original signature/CID/ciphertext queued.
            let file = self.outbox_attachment(&event);
            if file.is_some_and(|file| {
                file.chunks
                    .iter()
                    .chain(file.thumb.iter())
                    .any(|chunk| self.store.upload_pending(&chunk.blob).unwrap_or(true))
            }) {
                blocked.insert(event.space.clone());
                continue;
            }
            if matches!(&event.body, EventBody::MessagePosted { media: Some(_), .. }) {
                let Some(log) = self.logs.get(&event.space) else {
                    blocked.insert(event.space.clone());
                    continue;
                };
                let confirmed = media_dependencies
                    .entry(event.space.clone())
                    .or_insert_with(|| ConfirmedMediaDependencies::from_log(self, log).ok());
                if !confirmed
                    .as_ref()
                    .is_some_and(|confirmed| confirmed.allows(&event))
                {
                    // Keep the same Space's sealed generations behind this head;
                    // skipping it could outrun OpenMLS's existing reorder window.
                    blocked.insert(event.space.clone());
                    continue;
                }
            }
            let Some(mut env) = self.outgoing_envelope_with(&event, &mut ready) else {
                continue;
            };
            if let Some(bytes) = env.recovery() {
                let Some(reference) = event_log::recovery::RecoveryRef::parse(bytes) else {
                    continue;
                };
                if !self
                    .store
                    .recovery_upload_confirmed(&reference.blob)
                    .unwrap_or(false)
                {
                    continue;
                }
            }
            if env.sealed_kind() == Some(SealedKind::Application) {
                let n = in_flight.get(env.space()).copied().unwrap_or(0);
                if n >= Self::APPLICATION_IN_FLIGHT {
                    blocked.insert(env.space().to_string());
                    continue;
                }
                *in_flight.entry(env.space().to_string()).or_default() += 1;
            }
            if !env.is_sealed() {
                env.invite = self
                    .store
                    .meta(&format!("invite:{}", p.client_id))
                    .ok()
                    .flatten();
            }
            out.push(env);
        }
        out
    }

    /// Our queued Welcomes that wait behind a commit of ours still in the outbox.
    pub(super) fn welcomes_behind_commits(&self) -> HashSet<String> {
        let mut committing: HashSet<String> = HashSet::new();
        let mut held = HashSet::new();
        for (client_id, space, kind) in self.store.outbox_handshakes().unwrap_or_default() {
            if kind == SealedKind::Commit.name() {
                committing.insert(space);
            } else if kind == SealedKind::Welcome.name() && committing.contains(&space) {
                held.insert(client_id);
            }
        }
        held
    }

    pub fn outbox_len(&self) -> u64 {
        self.store.outbox_len().unwrap_or(0)
    }
}
