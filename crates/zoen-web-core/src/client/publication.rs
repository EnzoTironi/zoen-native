//! Whole operations on the existing Client; no additional state owner.
use super::{
    Client, ClientFrame, DocumentResponse, EventBody, Identity, Op, OpenMlsProvider, Privacy, Role,
    Signer, Update, PACKAGES_REQUEST,
};

impl<S: Signer> Client<S> {
    ///
    /// # Errors
    /// Returns an error if pending profile, membership or linking work, outbox
    /// resealing, or projection of the client update fails.
    pub fn maintenance(&mut self, now: i64) -> Result<Update, String> {
        self.view_now_ms = now;
        self.update(Vec::new())
    }

    ///
    /// # Errors
    /// Returns an error if profile, membership or linking frames cannot be
    /// prepared, an outbox event cannot be resealed, MLS storage cannot be read,
    /// or the saved session or document projection cannot be produced.
    pub fn update(&mut self, mut frames: Vec<Vec<u8>>) -> Result<Update, String> {
        self.profile_frames(&mut frames)?;
        self.membership_frames(&mut frames)?;
        self.linking_frames(&mut frames)?;
        self.reseal_outbox()?;
        if let Some(packages) = self
            .saved
            .packages
            .as_ref()
            .filter(|_| !self.packages_in_flight)
        {
            frames.insert(
                0,
                ClientFrame::Req {
                    id: PACKAGES_REQUEST,
                    op: Op::PublishKeyPackages {
                        packages: packages.single.clone(),
                        last_resort: packages.last_resort.clone(),
                    },
                }
                .encode(),
            );
            self.packages_in_flight = true;
        }
        self.publish_outbox(&mut frames)?;
        self.saved.logs = self
            .logs
            .iter()
            .map(|(space, log)| (space.clone(), log.events().to_vec()))
            .collect();
        self.saved.storage = self
            .provider
            .storage()
            .values
            .read()
            .map_err(|_| "MLS storage lock failed")?
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let snapshot = serde_json::to_string(&self.saved).map_err(|e| e.to_string())?;
        Ok(Update {
            snapshot,
            frames,
            cursor_repair: self.cursor_repair_status()?,
            history_ready: std::mem::take(&mut self.history_ready),
            conversations: self.conversations(),
            maintenance_ms: self
                .membership_deadline()
                .into_iter()
                .chain(self.profile_deadline())
                .chain(self.linking_deadline())
                .min()
                .map(|deadline| {
                    u32::try_from(deadline.saturating_sub(self.view_now_ms).clamp(50, 60_000))
                })
                .transpose()
                .map_err(|_| "maintenance deadline exceeds browser limit")?,
        })
    }

    fn reseal_outbox(&mut self) -> Result<(), String> {
        let reseal = self
            .saved
            .outbox
            .iter()
            .filter_map(|(id, pending)| {
                let event = pending.inner.as_ref()?;
                let epoch = self.ready_epoch(&event.space).ok()?;
                (pending.failure.is_none() && pending.epoch != Some(epoch))
                    .then(|| (id.clone(), event.clone(), epoch))
            })
            .collect::<Vec<_>>();
        for (id, event, epoch) in reseal {
            let mut pending = self.seal_inner(&event, epoch)?;
            let retained = self
                .saved
                .outbox
                .get(&id)
                .ok_or("outbox event missing during reseal")?;
            pending.blob_uploads.clone_from(&retained.blob_uploads);
            pending.order = retained.order;
            self.sent.remove(&id);
            self.saved.outbox.insert(id, pending);
        }
        Ok(())
    }

    fn publish_outbox(&mut self, frames: &mut Vec<Vec<u8>>) -> Result<(), String> {
        let mut outgoing = self
            .saved
            .outbox
            .iter()
            .filter(|(_, pending)| pending.failure.is_none() && pending.blob_uploads.is_empty())
            .filter(|(_, pending)| {
                pending
                    .after_commit
                    .as_ref()
                    .is_none_or(|id| !self.saved.outbox.contains_key(id))
            })
            .collect::<Vec<_>>();
        outgoing.sort_by_key(|(id, pending)| (pending.order.unwrap_or(0), *id));
        for (id, pending) in outgoing {
            if self.sent.contains(id)
                || (!self.synced
                    && matches!(
                        pending.envelope()?.body(),
                        Some(EventBody::ProfileKeyShared { .. })
                    ))
            {
                continue;
            }
            if let Some(inner) = &pending.inner {
                let encrypted = self
                    .metadata(&inner.space)
                    .is_some_and(|(_, _, privacy)| privacy == Privacy::EndToEnd);
                if !self.synced
                    || encrypted
                        && (pending.epoch.is_none()
                            || self.ready_epoch(&inner.space).ok() != pending.epoch)
                {
                    continue;
                }
            }
            frames.push(
                ClientFrame::Publish {
                    env: pending.envelope()?,
                }
                .encode(),
            );
            self.sent.insert(id.clone());
        }
        Ok(())
    }

    /// Delivered watermarks for every Space whose newest event this device now holds.
    pub(super) fn delivered(&mut self) -> Vec<Vec<u8>> {
        let me = self.account.identity.clone();
        let mut frames = Vec::new();
        for (space, log) in &self.logs {
            let Some(last) = log.next_seq().checked_sub(1) else {
                continue;
            };
            let mine = self
                .saved
                .receipts
                .entry(space.clone())
                .or_default()
                .entry(me.clone())
                .or_default();
            mine.0 = mine.0.max(last);
            let announced = self.announced_receipts.get(space).copied();
            if announced.is_none_or(|mark| mark.0 < mine.0) {
                frames.push(
                    ClientFrame::Ephemeral {
                        space: space.clone(),
                        kind: protocol::EphemeralKind::Delivered { seq: mine.0 },
                    }
                    .encode(),
                );
            }
            if mine.1 > 0 && announced.is_none_or(|mark| mark.1 < mine.1) {
                frames.push(
                    ClientFrame::Ephemeral {
                        space: space.clone(),
                        kind: protocol::EphemeralKind::Read { seq: mine.1 },
                    }
                    .encode(),
                );
            }
            self.announced_receipts.insert(space.clone(), *mine);
        }
        frames
    }

    /// I have read this conversation up to its newest event: tell my devices and the
    /// other members (one watermark, like native).
    ///
    /// # Errors
    /// Returns an error if the resulting client update cannot be produced.
    pub fn mark_read(&mut self, space: &str) -> Result<Update, String> {
        let Some(last) = self
            .logs
            .get(space)
            .and_then(|log| log.next_seq().checked_sub(1))
        else {
            return self.update(Vec::new());
        };
        let me = self.account.identity.clone();
        let mine = self
            .saved
            .receipts
            .entry(space.into())
            .or_default()
            .entry(me)
            .or_default();
        *mine = (mine.0.max(last), mine.1.max(last));
        self.update(vec![ClientFrame::Ephemeral {
            space: space.into(),
            kind: protocol::EphemeralKind::Read { seq: last },
        }
        .encode()])
    }

    ///
    /// # Errors
    /// This operation currently returns no errors; callers receive an empty frame
    /// when disconnected or lacking permission to send typing notifications.
    pub fn typing_frame(&self, space: &str, typing: bool) -> Result<Vec<u8>, String> {
        if !self.synced
            || self
                .roster(space)
                .get(&self.account.identity)
                .is_none_or(|role| *role == Role::Reader)
        {
            return Ok(Vec::new());
        }
        Ok(ClientFrame::Ephemeral {
            space: space.into(),
            kind: if typing {
                protocol::EphemeralKind::Typing
            } else {
                protocol::EphemeralKind::StoppedTyping
            },
        }
        .encode())
    }

    pub(super) fn put_directory_profiles(&mut self, profiles: Vec<Identity>) {
        for mut profile in profiles {
            if profile.id == self.account.identity {
                continue;
            }
            if event_log::parse_device_key(&profile.id).is_none()
                || !event_log::agent_owner::profile_authorized(&profile)
            {
                continue;
            }
            if self.saved.profiles.get(&profile.id).is_some_and(|known| {
                known.owner_proof.is_some()
                    && event_log::agent_owner::profile_authorized(known)
                    && (known.kind != profile.kind
                        || known.owner != profile.owner
                        || known.owner_proof != profile.owner_proof)
            }) {
                continue;
            }
            // Directory entries never reveal a person's private fields. Those are
            // overlaid only after the shared encrypted-profile codec authenticates them.
            if profile.kind == types::IdentityKind::Person {
                profile.name.clear();
                profile.bio.clear();
            }
            self.saved.profiles.insert(profile.id.clone(), profile);
        }
        self.overlay_saved_profiles();
    }

    ///
    /// # Errors
    /// Returns an error for oversized or malformed profile responses, too many
    /// profiles, failure to project a profile, or client update failure.
    pub fn directory_profiles(&mut self, encoded: &str) -> Result<DocumentResponse, String> {
        if encoded.len() > 1 << 20 {
            return Err("Resposta de perfis grande demais.".into());
        }
        let profiles: Vec<Identity> =
            serde_json::from_str(encoded).map_err(|error| error.to_string())?;
        if profiles.len() > 500 {
            return Err("Perfis demais em uma consulta.".into());
        }
        let ids = profiles
            .iter()
            .map(|profile| profile.id.clone())
            .collect::<Vec<_>>();
        self.put_directory_profiles(profiles);
        let result = serde_json::Value::Array(
            ids.iter()
                .filter(|id| **id == self.account.identity || self.saved.profiles.contains_key(*id))
                .map(|id| self.persona_value(id))
                .collect(),
        );
        Ok(DocumentResponse {
            result,
            update: Some(self.update(Vec::new())?),
            downloads: Vec::new(),
            uploads: Vec::new(),
        })
    }
}
