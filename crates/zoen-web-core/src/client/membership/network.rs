use super::*;
impl<S: Signer> Client<S> {
    pub(in crate::client) fn membership_reply(
        &mut self,
        id: u64,
        result: &Result<Reply, String>,
        outgoing: &mut Vec<Vec<u8>>,
    ) -> Result<bool, String> {
        if (DEVICES_REQUEST_BASE..=DEVICES_REQUEST_BASE + u64::from(u32::MAX)).contains(&id) {
            if self.membership_net.devices_requested && id == self.membership_net.devices_request {
                self.device_directory_reply(result, outgoing)?;
            }
            return Ok(true);
        }
        if id != CLAIM_REQUEST {
            return Ok(false);
        }
        let Some((space, operation, _)) = self.membership_net.claiming.take() else {
            return Ok(true);
        };
        if let Some(claim) = self
            .saved
            .membership
            .claims
            .get_mut(&space)
            .filter(|claim| claim.operation == operation)
        {
            match result {
                Ok(Reply::KeyPackages(records)) => claim.records = Some(records.clone()),
                Err(error) if error == protocol::KEY_PACKAGE_CLAIM_EXPIRED => {
                    self.saved.membership.claims.remove(&space);
                    self.membership_net.clock = None;
                }
                _ => {}
            }
        }
        self.membership_net
            .retry
            .insert(space, self.view_now_ms.saturating_add(5000));
        Ok(true)
    }
    pub(in crate::client) fn creating_views(&self) -> Vec<ConversationView> {
        self.saved
            .outbox
            .values()
            .filter_map(|pending| {
                if pending.failure.is_some() {
                    return None;
                }
                let envelope = pending.envelope().ok()?;
                let EventBody::SpaceCreated {
                    title,
                    kind,
                    privacy,
                } = envelope.body()?
                else {
                    return None;
                };
                if self.logs.contains_key(envelope.space()) {
                    return None;
                }
                let mut roster = self
                    .saved
                    .membership
                    .creates
                    .get(envelope.space())
                    .into_iter()
                    .flatten()
                    .map(|identity| (identity.clone(), Role::Member))
                    .collect::<BTreeMap<_, _>>();
                roster.insert(self.account.identity.clone(), Role::Owner);
                let title = if kind == SpaceKind::Direct {
                    roster
                        .keys()
                        .find(|id| **id != self.account.identity)
                        .and_then(|id| self.saved.profiles.get(id))
                        .map(|person| {
                            if person.name.is_empty() {
                                format!("@{}", person.handle)
                            } else {
                                person.name.clone()
                            }
                        })
                        .unwrap_or_else(|| "Conversa".into())
                } else {
                    title
                };
                Some(ConversationView {
                    id: envelope.space().into(),
                    title,
                    kind: match kind {
                        SpaceKind::Direct => "direct",
                        SpaceKind::Group => "group",
                        SpaceKind::Community => "community",
                        SpaceKind::Personal => return None,
                    },
                    privacy,
                    profiles: roster
                        .keys()
                        .filter_map(|id| self.conversation_profile(id))
                        .collect(),
                    members: roster.len(),
                    roster,
                    can_send: false,
                    waiting: Some("Criando a conversa…".into()),
                    integrity_error: None,
                    unread: 0,
                    mentions: 0,
                    unreadable: 0,
                    messages: Vec::new(),
                    event_count: "0".into(),
                    prefs: ChatPreferencesView {
                        pinned: false,
                        archived: false,
                        muted_until_ms: None,
                    },
                })
            })
            .collect()
    }

    pub(in crate::client) fn membership_deadline(&self) -> Option<i64> {
        self.membership_net
            .turns
            .values()
            .chain(self.membership_net.retry.values())
            .copied()
            .chain(std::iter::once(self.membership_net.devices_due))
            .chain(
                self.membership_net
                    .claiming
                    .iter()
                    .map(|(_, _, deadline)| *deadline),
            )
            .filter(|deadline| *deadline > self.view_now_ms)
            .min()
    }

    pub(in crate::client) fn membership_refused(&mut self, id: &str) -> Result<bool, String> {
        let Some(pending) = self.saved.outbox.get(id) else {
            return Ok(false);
        };
        let envelope = pending.envelope()?;
        let space = envelope.space().to_owned();
        match envelope.sealed_kind() {
            Some(SealedKind::Commit) => {
                // Restore the committed epoch before replaying the winner. User events
                // remain in the durable outbox and will be resealed against that epoch.
                self.device()?
                    .abandon(&space)
                    .map_err(|error| error.to_string())?;
                let dropped = self
                    .saved
                    .outbox
                    .iter()
                    .filter(|(other, pending)| {
                        *other == id || pending.after_commit.as_deref() == Some(id)
                    })
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>();
                for id in dropped {
                    self.saved.outbox.remove(&id);
                    self.saved.membership.welcomes.remove(&id);
                    self.sent.remove(&id);
                }
            }
            Some(SealedKind::Welcome) => {
                let stranded = self
                    .saved
                    .membership
                    .welcomes
                    .remove(id)
                    .unwrap_or_default();
                self.saved
                    .membership
                    .stranded
                    .entry(space.clone())
                    .or_default()
                    .extend(stranded);
                self.saved.outbox.remove(id);
                self.sent.remove(id);
            }
            _ => return Ok(false),
        }
        self.saved.membership.claims.remove(&space);
        self.membership_net
            .retry
            .insert(space, self.view_now_ms.saturating_add(500));
        Ok(true)
    }

    pub(in crate::client) fn membership_frames(
        &mut self,
        frames: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        if !self.synced {
            return Ok(());
        }
        if self
            .membership_net
            .claiming
            .as_ref()
            .is_some_and(|(_, _, deadline)| *deadline <= self.view_now_ms)
        {
            self.membership_net.claiming = None;
        }
        if self.membership_net.devices_requested
            && self.membership_net.devices_due <= self.view_now_ms
        {
            self.device_directory_cancel(frames);
            self.membership_net.devices_requested = false;
        }
        if !self.membership_net.devices_requested
            && self.membership_net.devices_due <= self.view_now_ms
            && self.logs.keys().any(|space| {
                self.metadata(space).is_some_and(|(_, kind, privacy)| {
                    kind != SpaceKind::Personal && privacy == Privacy::EndToEnd
                })
            })
        {
            let op = self.device_directory_begin()?;
            frames.push(
                ClientFrame::Req {
                    id: self.membership_net.devices_request,
                    op,
                }
                .encode(),
            );
            self.membership_net.devices_requested = true;
            self.membership_net.devices_due = self.view_now_ms.saturating_add(15_000);
        }
        for space in self.logs.keys().cloned().collect::<Vec<_>>() {
            if !self.metadata(&space).is_some_and(|(_, kind, privacy)| {
                kind != SpaceKind::Personal && privacy == Privacy::EndToEnd
            }) {
                continue;
            }
            self.settle_membership(&space)?;
            let Some(owed) = self.owed_membership(&space)? else {
                self.membership_net.turns.remove(&space);
                self.membership_net.retry.remove(&space);
                continue;
            };
            let ahead = self
                .roster(&space)
                .iter()
                .filter(|(identity, role)| {
                    matches!(role, Role::Owner | Role::Admin)
                        && identity.as_str() < self.account.identity.as_str()
                })
                .count() as i64;
            if ahead > 0 {
                let due = self
                    .membership_net
                    .turns
                    .entry(space.clone())
                    .or_insert(self.view_now_ms.saturating_add(ahead.saturating_mul(400)));
                if *due > self.view_now_ms {
                    continue;
                }
            }
            if self
                .membership_net
                .retry
                .get(&space)
                .is_some_and(|at| *at > self.view_now_ms)
                && self
                    .saved
                    .membership
                    .claims
                    .get(&space)
                    .is_none_or(|claim| claim.records.is_none())
            {
                continue;
            }
            if self.recovery_recipients(&space).is_err() {
                continue;
            }
            if owed.adds_nobody() {
                if self.stage_membership(&space, &[], &owed.remove).is_err() {
                    self.membership_net
                        .retry
                        .insert(space.clone(), self.view_now_ms.saturating_add(5000));
                }
                continue;
            }
            if self.membership_net.claiming.is_some() {
                continue;
            }
            let epoch = self
                .device()?
                .epoch(&space)
                .map_err(|error| error.to_string())?;
            let saved = self
                .saved
                .membership
                .claims
                .get(&space)
                .filter(|claim| {
                    claim.device == self.account.device
                        && claim.epoch == epoch
                        && claim.intent == owed
                })
                .cloned();
            let claim = if let Some(saved) = saved {
                saved
            } else {
                let Some(clock) = self.membership_net.clock else {
                    continue;
                };
                if self
                    .membership_net
                    .retry
                    .get(&space)
                    .is_some_and(|at| *at > self.view_now_ms)
                {
                    continue;
                }
                let mut random = [0; 16];
                getrandom::getrandom(&mut random).map_err(|error| error.to_string())?;
                let mut value = ((clock.milliseconds() as u128) << 80)
                    | (u128::from_be_bytes(random) & ((1u128 << 80) - 1));
                let alphabet = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
                let mut operation = [0; 26];
                for byte in operation.iter_mut().rev() {
                    *byte = alphabet[(value & 31) as usize];
                    value >>= 5;
                }
                Claim {
                    operation: String::from_utf8(operation.to_vec())
                        .map_err(|error| error.to_string())?,
                    device: self.account.device.clone(),
                    epoch,
                    intent: owed.clone(),
                    targets: owed.to_claim(),
                    records: None,
                }
            };
            self.saved
                .membership
                .claims
                .insert(space.clone(), claim.clone());
            if let Some(records) = claim.records {
                let records = records
                    .into_iter()
                    .filter(|record| {
                        mls::key_package_leaf(&record.data).is_ok_and(|leaf| {
                            leaf.identity == record.identity
                                && leaf.device == record.device
                                && claim.targets.contains(&leaf.identity)
                                && (owed.add.contains(&leaf.identity)
                                    || owed
                                        .add_devices
                                        .contains(&(leaf.identity.clone(), leaf.device.clone())))
                                && !owed
                                    .remove
                                    .contains(&leaf_name(&record.identity, &record.device))
                        })
                    })
                    .collect::<Vec<_>>();
                if owed.add.iter().any(|id| {
                    claim.targets.contains(id)
                        && !records.iter().any(|record| &record.identity == id)
                }) {
                    self.membership_net.stuck.insert(space.clone());
                }
                let packages = records
                    .into_iter()
                    .map(|record| record.data)
                    .collect::<Vec<_>>();
                if packages.is_empty() && owed.remove.is_empty() {
                    self.saved.membership.claims.remove(&space);
                    continue;
                }
                if self
                    .stage_membership(&space, &packages, &owed.remove)
                    .is_err()
                {
                    // Bad/noncontributory member keys cannot tear down the account.
                    self.saved.membership.claims.remove(&space);
                    self.membership_net
                        .retry
                        .insert(space.clone(), self.view_now_ms.saturating_add(5000));
                }
            } else {
                self.membership_net.claiming = Some((
                    space.clone(),
                    claim.operation.clone(),
                    self.view_now_ms.saturating_add(15_000),
                ));
                frames.push(
                    ClientFrame::Req {
                        id: CLAIM_REQUEST,
                        op: Op::ClaimKeyPackages {
                            ids: claim.targets,
                            operation_id: Some(claim.operation),
                        },
                    }
                    .encode(),
                );
            }
        }
        Ok(())
    }
}
