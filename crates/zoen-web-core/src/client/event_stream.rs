//! Whole operations on the existing Client; no additional state owner.
use super::{
    inner_event, linking, profile, BTreeSet, Client, ClientFrame, Cursor, Duration, Event,
    EventBody, GroupState, Leaf, Op, Opened, Packages, PendingEnvelope, Privacy, Reply, SealedKind,
    Seen, Sequenced, ServerFrame, Signer, SpaceKind, SpaceLog, Update, PACKAGES_REQUEST,
    PROFILES_REQUEST,
};

impl<S: Signer> Client<S> {
    ///
    /// # Errors
    /// Returns an error if key-package preparation or the initial client update fails.
    pub fn bootstrap(&mut self) -> Result<Update, String> {
        self.bootstrap_at(Duration::ZERO)
    }

    ///
    /// # Errors
    /// Returns an error for an invalid reconciliation clock, failed key-package
    /// preparation, or failure to produce the initial client update.
    pub fn bootstrap_at(&mut self, now: Duration) -> Result<Update, String> {
        Self::validate_repair_clock(now)?;
        self.repair_now = now;
        self.cursor_repair = Some(protocol::cursor_repair::CursorRepair::new(
            protocol::cursor_repair::CursorRepairPolicy::DEFAULT,
            now,
        ));
        self.synced = false;
        self.sent.clear();
        self.announced_receipts.clear();
        self.deferred_receipts.clear();
        self.packages_in_flight = false;
        self.profile_net = profile::Network::default();
        self.membership_net.restart();
        self.linking_net = linking::Network::default();
        if !self.saved.packages_published && self.saved.packages.is_none() {
            self.prepare_packages(true)?;
        }
        let cursors = self
            .logs
            .iter()
            .map(|(space, log)| Cursor {
                space: space.clone(),
                next_seq: log.next_seq(),
            })
            .collect();
        self.update(vec![ClientFrame::Sync { cursors, all: true }.encode()])
    }

    pub(super) fn prepare_packages(&mut self, first: bool) -> Result<(), String> {
        let mut single = self
            .device()?
            .key_packages(32, first)
            .map_err(|e| e.to_string())?;
        let last_resort = if first { single.pop() } else { None };
        self.saved.packages = Some(Packages {
            single,
            last_resort,
        });
        Ok(())
    }

    ///
    /// # Errors
    /// Returns an error if the frame cannot be decoded, authenticated or applied,
    /// or if reconciliation or the resulting client update fails.
    pub fn receive(&mut self, bytes: &[u8], now_ms: i64) -> Result<Update, String> {
        self.receive_at(bytes, now_ms, self.repair_now)
    }

    ///
    /// # Errors
    /// Returns an error for invalid or backwards reconciliation time, an invalid
    /// frame, failed authentication or application, or failure to produce the update.
    pub fn receive_at(
        &mut self,
        bytes: &[u8],
        now_ms: i64,
        repair_now: Duration,
    ) -> Result<Update, String> {
        self.note_repair_clock(repair_now)?;
        self.view_now_ms = now_ms;
        let mut extra = Vec::new();
        match ServerFrame::decode(bytes).map_err(|e| e.to_string())? {
            ServerFrame::Res { id, result } if self.linking_reply(id, &result) => {}
            ServerFrame::Res { id, result } if self.membership_reply(id, &result, &mut extra) => {}
            ServerFrame::Res { id, result } if self.profile_reply(id, &result)? => {}
            ServerFrame::DeviceMessage { from, to, sealed } => {
                self.history_ready |= self.history_hint(&from, &to, &sealed);
                self.linked_device_message(&from, &to, &sealed, &mut extra)?;
            }
            ServerFrame::ProfileChanged { identity, .. } => {
                self.profile_changed(&identity);
            }
            ServerFrame::Event { ev } => {
                if let Some(update) =
                    self.receive_ordered_event(ev, now_ms, repair_now, &mut extra)?
                {
                    return Ok(update);
                }
            }
            ServerFrame::Ephemeral { space, from, kind } => {
                let mark = match kind {
                    protocol::EphemeralKind::Read { seq } => Some((seq, seq)),
                    protocol::EphemeralKind::Delivered { seq } => Some((seq, 0)),
                    _ => None,
                };
                if let Some((delivered, read)) = mark {
                    self.receive_receipt(&space, &from, delivered, read)?;
                }
            }
            ServerFrame::SyncDone => self.complete_sync(now_ms, repair_now, &mut extra)?,
            ServerFrame::Joined { space } => {
                self.repair_submitted()?;
                extra.push(
                    ClientFrame::Sync {
                        cursors: vec![Cursor { space, next_seq: 0 }],
                        all: false,
                    }
                    .encode(),
                );
            }
            ServerFrame::KeyPackagesLow { device, .. }
                if device == self.account.device && self.saved.packages.is_none() =>
            {
                self.prepare_packages(!self.saved.packages_published)?;
            }
            ServerFrame::Res {
                id: PACKAGES_REQUEST,
                result: Ok(Reply::Done),
            } => {
                self.saved.packages = None;
                self.saved.packages_published = true;
                self.packages_in_flight = false;
            }
            ServerFrame::Res {
                id: PACKAGES_REQUEST,
                result: Err(error),
            } => return Err(format!("Could not publish encrypted device keys: {error}")),
            ServerFrame::Res {
                result: Ok(Reply::Profiles(profiles)),
                ..
            } => {
                self.put_directory_profiles(profiles);
            }
            ServerFrame::Rejected {
                client_id,
                reason,
                permanent,
                ..
            } => {
                if permanent && self.membership_refused(&client_id)? {
                    return self.update(extra);
                }
                if let Some(pending) = self.saved.outbox.get_mut(&client_id) {
                    if permanent {
                        pending.failure = Some(reason);
                    } else if let Some(epoch) = pending.epoch {
                        self.saved
                            .blocked_epochs
                            .insert(pending.envelope()?.space().to_string(), epoch);
                    }
                }
                self.sent.remove(&client_id);
            }
            _ => {}
        }
        self.flush_receipts();
        self.update(extra)
    }

    fn receive_ordered_event(
        &mut self,
        ev: Sequenced,
        now_ms: i64,
        repair_now: Duration,
        extra: &mut Vec<Vec<u8>>,
    ) -> Result<Option<Update>, String> {
        let space = ev.env.space().to_string();
        let next = self.logs.get(&space).map_or(0, SpaceLog::next_seq);
        if ev.seq > next {
            self.repair_submitted()?;
            return self
                .update(vec![ClientFrame::Sync {
                    cursors: vec![Cursor {
                        space,
                        next_seq: next,
                    }],
                    all: false,
                }
                .encode()])
                .map(Some);
        }
        let author = ev.env.author().to_string();
        self.ingest(ev, now_ms)?;
        if self
            .logs
            .get(&space)
            .is_some_and(|log| log.next_seq() > next)
        {
            if let Some(repair) = &mut self.cursor_repair {
                repair.progressed(repair_now);
            }
        }
        if self.synced {
            extra.extend(self.delivered());
            if !self.saved.profiles.contains_key(&author) {
                extra.push(
                    ClientFrame::Req {
                        id: PROFILES_REQUEST,
                        op: Op::Profiles { ids: vec![author] },
                    }
                    .encode(),
                );
            }
        }
        Ok(None)
    }

    fn complete_sync(
        &mut self,
        now_ms: i64,
        repair_now: Duration,
        extra: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        if let Some(repair) = &mut self.cursor_repair {
            repair.completed(repair_now);
        }
        self.synced = true;
        extra.extend(self.delivered());
        self.ask_rejoin(now_ms)?;
        let ids = self
            .logs
            .values()
            .flat_map(|log| log.events().iter().map(|event| event.author.clone()))
            .filter(|identity| !self.saved.profiles.contains_key(identity))
            .collect::<BTreeSet<_>>();
        if !ids.is_empty() {
            extra.push(
                ClientFrame::Req {
                    id: PROFILES_REQUEST,
                    op: Op::Profiles {
                        ids: ids.into_iter().collect(),
                    },
                }
                .encode(),
            );
        }
        Ok(())
    }

    pub(super) fn ask_rejoin(&mut self, now_ms: i64) -> Result<(), String> {
        let mut missing = Vec::new();
        for space in self.logs.keys() {
            if self.metadata(space).is_some_and(|(_, kind, privacy)| {
                privacy == Privacy::EndToEnd && kind != SpaceKind::Personal
            }) && self.roster(space).contains_key(&self.account.identity)
                && !self.saved.joining.contains(space)
                && self
                    .device()?
                    .group_state(space)
                    .map_err(|e| e.to_string())?
                    == GroupState::Missing
            {
                missing.push(space.clone());
            }
        }
        for space in missing {
            let pending = self.clear_event(
                &space,
                &EventBody::DeviceJoining {
                    device: self.account.device.clone(),
                },
                now_ms,
            )?;
            let id = pending.envelope()?.client_id().to_string();
            self.saved.outbox.insert(id, pending);
            self.saved.joining.insert(space);
        }
        Ok(())
    }

    pub(super) fn ingest(&mut self, sequenced: Sequenced, now_ms: i64) -> Result<(), String> {
        let envelope = &sequenced.env;
        let space = envelope.space().to_string();
        let mut log = self
            .logs
            .get(&space)
            .cloned()
            .unwrap_or_else(|| SpaceLog::new(&space));
        if sequenced.seq < log.next_seq() {
            let retained = log
                .events()
                .get(
                    usize::try_from(sequenced.seq)
                        .map_err(|_| "event sequence exceeds platform limit")?,
                )
                .ok_or("invalid event sequence")?;
            if retained.hash != sequenced.hash {
                return Err("The relay sent a conflicting conversation history.".into());
            }
            return Ok(());
        }
        envelope.verify().map_err(|e| e.to_string())?;
        let outer = event_log::event_from_content(
            envelope.content().to_vec(),
            envelope.sig.clone(),
            envelope.cert.clone(),
            sequenced.seq,
            sequenced.prev.clone(),
            sequenced.hash.clone(),
        )
        .map_err(|e| e.to_string())?;
        let mut outer_check = log.clone();
        outer_check
            .accept(outer.clone())
            .map_err(|e| e.to_string())?;
        let roster = self.roster(&space).keys().cloned().collect();
        let admitted = Leaf {
            identity: envelope.author().into(),
            device: envelope.device().unwrap_or(envelope.author()).into(),
            cert: envelope.cert.clone().unwrap_or_default(),
        };
        let own = self
            .saved
            .outbox
            .get(envelope.client_id())
            .filter(|pending| {
                pending.content == envelope.content()
                    && pending.sig == envelope.sig
                    && envelope.cert.as_deref() == Some(pending.cert.as_str())
            })
            .cloned();
        let (opened, landed) =
            self.open_ordered_event(&sequenced, &roster, &admitted, own.as_ref())?;
        let event = if let Some(mut inner) = opened {
            inner.seq = sequenced.seq;
            inner.prev.clone_from(&sequenced.prev);
            inner.hash.clone_from(&sequenced.hash);
            inner.sealed_wire = Some(envelope.wire_hash());
            inner
        } else {
            outer
        };
        log.accept(event.clone()).map_err(|e| e.to_string())?;
        self.logs.insert(space.clone(), log);
        self.note_profile_event(&event);
        self.note_membership_event(&event)?;
        if own.is_some() {
            self.saved.outbox.remove(envelope.client_id());
            self.sent.remove(envelope.client_id());
        }
        if matches!(&event.body, EventBody::MemberRemoved { identity } if identity == &self.account.identity)
        {
            self.device()?.forget(&space).map_err(|e| e.to_string())?;
            self.saved.joining.remove(&space);
        }
        self.record_ingest_checkpoint(&event, landed, sequenced, now_ms)
    }

    fn record_ingest_checkpoint(
        &mut self,
        event: &Event,
        landed: bool,
        sequenced: Sequenced,
        now_ms: i64,
    ) -> Result<(), String> {
        let space = event.space.clone();
        if let EventBody::Checkpoint { epoch, digest, .. } = &event.body {
            if let Ok((ours, expected)) = self.device()?.checkpoint(&space) {
                if ours == *epoch && expected != *digest {
                    self.saved.integrity_errors.insert(space.clone(), "The encrypted conversation has conflicting membership. Open it on your phone.".into());
                }
            }
        }
        if landed {
            self.saved.joining.remove(&space);
            if let Ok((epoch, digest)) = self.device()?.checkpoint(&space) {
                let checkpoint = self.clear_event(
                    &space,
                    &EventBody::Checkpoint {
                        upto: Seen {
                            seq: sequenced.seq,
                            hash: sequenced.hash,
                        },
                        epoch,
                        digest,
                    },
                    now_ms,
                )?;
                self.saved
                    .outbox
                    .insert(checkpoint.envelope()?.client_id().to_string(), checkpoint);
            }
        }
        Ok(())
    }

    fn open_ordered_event(
        &self,
        sequenced: &Sequenced,
        roster: &BTreeSet<String>,
        admitted: &Leaf,
        own: Option<&PendingEnvelope>,
    ) -> Result<(Option<Event>, bool), String> {
        let envelope = &sequenced.env;
        let space = envelope.space();
        let mut opened = None;
        let mut landed = false;
        if !envelope.is_pruned() {
            if let Some((kind, ciphertext)) = envelope.sealed_data() {
                let device = self.device()?;
                match kind {
                    SealedKind::Application => {
                        if let Some(inner) = own.and_then(|pending| pending.inner.clone()) {
                            opened = Some(inner);
                        } else {
                            match device.open(space, ciphertext, roster, admitted) {
                                Ok(Opened::Application { plaintext, from }) => {
                                    opened = inner_event(sequenced, &plaintext, &from);
                                }
                                Err(mls::MlsError::Storage(error)) => return Err(error),
                                Ok(_) | Err(_) => {}
                            }
                        }
                    }
                    SealedKind::Welcome
                        if device.group_state(space).map_err(|e| e.to_string())?
                            == GroupState::Missing =>
                    {
                        match device.join(space, ciphertext, roster) {
                            Ok(joined) => landed = joined,
                            Err(mls::MlsError::Storage(error)) => return Err(error),
                            Err(_) => {}
                        }
                    }
                    SealedKind::Commit => match device.open(space, ciphertext, roster, admitted) {
                        Ok(Opened::Commit { .. }) => landed = true,
                        Err(mls::MlsError::Storage(error)) => return Err(error),
                        Ok(_) | Err(_) => {}
                    },
                    _ => {}
                }
            }
        }
        Ok((opened, landed))
    }
}
