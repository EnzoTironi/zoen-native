//! Saved Messages reuses the ordinary signed log, outbox, MLS and device policy.
use crate::{
    engine::{Engine, R},
    Client, CoreError, SpaceKindDto, SpaceSummary,
};
use protocol::{saved_messages::Genesis, Envelope, Op, Reply};
use types::{Event, EventBody, Privacy, Role, SpaceKind};

fn refused(reason: &str) -> CoreError {
    CoreError::Invalid {
        reason: reason.into(),
    }
}

impl Engine {
    pub(crate) fn saved_messages_id(&self, identity: &str, device: &str) -> R<String> {
        self.native_surface_owner(identity, device)?;
        types::saved_messages::space_id(identity)
            .ok_or_else(|| refused("invalid saved messages owner"))
    }
    pub(crate) fn saved_messages_canonical(&self, space: &str) -> bool {
        self.logs
            .get(space)
            .and_then(|log| log.events().first())
            .is_some_and(|event| {
                event.seq == 0 && types::saved_messages::genesis(space, &event.author, &event.body)
            })
    }
    pub(crate) fn saved_messages_space(&self, space: &str) -> bool {
        let Some(me) = self.me.as_deref() else {
            return false;
        };
        types::saved_messages::owned_by(space, me)
            && self.state.spaces.get(space).is_some_and(|s| {
                s.kind == SpaceKind::Direct
                    && s.privacy == Privacy::EndToEnd
                    && s.members == vec![(me.to_owned(), Role::Owner)]
            })
    }
    pub(crate) fn saved_messages_admit(
        &self,
        space: &str,
        author: &str,
        body: &EventBody,
    ) -> R<()> {
        if !types::saved_messages::reserved(space) {
            return Ok(());
        }
        if self.me.as_deref() != Some(author)
            || !types::saved_messages::owned_by(space, author)
            || matches!(
                body,
                EventBody::MemberAdded { .. } | EventBody::MemberRemoved { .. }
            )
        {
            return Err(refused("saved messages has only its owner"));
        }
        if matches!(body, EventBody::SpaceCreated { .. }) {
            if !types::saved_messages::genesis(space, author, body) {
                return Err(refused("invalid saved messages genesis"));
            }
        } else if !self.saved_messages_canonical(space) {
            return Err(refused("saved messages is still being prepared"));
        }
        Ok(())
    }
    fn saved_messages_pending(&self, space: &str) -> R<Vec<(store::OutboxHead, Event)>> {
        self.store
            .outbox_heads()?
            .into_iter()
            .filter(|head| head.space == space)
            .map(|head| {
                let event = self
                    .read_outbox_head(&head)
                    .map_err(|_| refused("saved messages retained original is unreadable"))?;
                Ok((head, event))
            })
            .collect()
    }
    fn prepare_saved_messages(&mut self, identity: &str, device: &str) -> R<Option<Genesis>> {
        let space = self.saved_messages_id(identity, device)?;
        if self.saved_messages_canonical(&space) {
            return Ok(None);
        }
        if !self.net.saved_messages_supported {
            return Err(refused("this relay does not support saved messages"));
        }
        let pending = self.saved_messages_pending(&space)?;
        if pending
            .iter()
            .any(|(_, event)| !matches!(event.body, EventBody::SpaceCreated { .. }))
        {
            return Err(refused("unexpected pending saved messages event"));
        }
        if pending.len() > 1 {
            return Err(refused("conflicting saved messages bootstrap"));
        }
        let original = if let Some((head, event)) = pending.into_iter().next() {
            let account = self
                .net
                .account
                .as_ref()
                .ok_or_else(|| refused("account"))?;
            if self
                .device()?
                .group_state(&space)
                .map_err(crate::mls::mls_err)?
                != ::mls::GroupState::Ready
            {
                return Err(refused("saved messages bootstrap keys require recovery"));
            }
            if head.failed
                || event.author != identity
                || event.device.as_deref() != Some(device)
                || event.cert.as_deref() != Some(account.cert.as_str())
                || !types::saved_messages::genesis(&space, identity, &event.body)
            {
                return Err(refused("saved messages bootstrap owner changed"));
            }
            event
        } else {
            if self
                .device()?
                .group_state(&space)
                .map_err(crate::mls::mls_err)?
                != ::mls::GroupState::Missing
            {
                return Err(refused("saved messages existing keys require recovery"));
            }
            self.store
                .conn()
                .execute_batch("SAVEPOINT saved_messages_prepare")
                .map_err(store::StoreError::from)?;
            let result: R<Event> = (|| {
                self.store.set_synced(&space)?;
                self.net.synced.insert(space.clone());
                // Both provider and outbox use this exact Store connection.
                // Tentative keys and the signed intent commit or roll back together.
                self.device()?
                    .create_group(&space)
                    .map_err(crate::mls::mls_err)?;
                let original = self.append(
                    &space,
                    identity,
                    EventBody::SpaceCreated {
                        title: String::new(),
                        kind: SpaceKind::Direct,
                        privacy: Privacy::EndToEnd,
                    },
                )?;
                self.store
                    .conn()
                    .execute_batch("RELEASE saved_messages_prepare")
                    .map_err(store::StoreError::from)?;
                Ok(original)
            })();
            match result {
                Ok(original) => original,
                Err(error) => {
                    let _ = self.store.conn().execute_batch(
                        "ROLLBACK TO saved_messages_prepare; RELEASE saved_messages_prepare",
                    );
                    self.reload()?;
                    return Err(error);
                }
            }
        };
        let env = Envelope::plain(&original)
            .map_err(|_| refused("invalid retained saved messages bootstrap"))?;
        env.verify()
            .map_err(|_| refused("invalid retained saved messages signature"))?;
        Ok(Some(Genesis::from_envelope(&env)))
    }
    /// Called inside canonical ingestion's existing SQLite transaction.
    pub(crate) fn saved_messages_superseded(&self, canonical: &Event) -> R<Vec<String>> {
        if canonical.seq != 0
            || !types::saved_messages::genesis(&canonical.space, &canonical.author, &canonical.body)
        {
            return Ok(Vec::new());
        }
        let Some(account) = self.net.account.as_ref() else {
            return Ok(Vec::new());
        };
        if account.identity != canonical.author {
            return Err(refused("foreign saved messages genesis"));
        }
        let mut ids = Vec::new();
        for (_, event) in self.saved_messages_pending(&canonical.space)? {
            if event.client_id == canonical.client_id
                || !matches!(event.body, EventBody::SpaceCreated { .. })
            {
                continue;
            }
            if event.author != canonical.author
                || !types::saved_messages::genesis(&event.space, &canonical.author, &event.body)
                || event.device.as_deref() != Some(account.device.as_str())
                || event.cert.as_deref() != Some(account.cert.as_str())
            {
                return Err(refused("conflicting saved messages bootstrap owner"));
            }
            ids.push(event.client_id);
        }
        if !ids.is_empty() {
            if canonical.device.as_deref() == Some(account.device.as_str()) {
                return Err(refused("conflicting saved messages bootstrap device"));
            }
            // The candidate never published MLS: discard only its tentative group,
            // in canonical ingestion's same transaction, then join the winner normally.
            let device = self.device()?;
            match device
                .group_state(&canonical.space)
                .map_err(crate::mls::mls_err)?
            {
                ::mls::GroupState::Ready => device
                    .forget(&canonical.space)
                    .map_err(crate::mls::mls_err)?,
                ::mls::GroupState::Missing => {}
                ::mls::GroupState::Pending => {
                    return Err(refused("saved messages bootstrap keys require recovery"))
                }
            }
        }
        Ok(ids)
    }
    fn saved_messages_ready(&mut self, space: &str) -> R<SpaceSummary> {
        if !self.saved_messages_space(space) || !self.saved_messages_canonical(space) {
            return Err(refused("saved messages is still being prepared"));
        }
        let account = self
            .net
            .account
            .as_ref()
            .ok_or_else(|| refused("account"))?;
        self.native_surface_owner(&account.identity, &account.device)?;
        let summary = self.space_summary(space)?;
        if summary.kind != SpaceKindDto::Saved {
            return Err(refused("invalid saved messages projection"));
        }
        Ok(summary)
    }
}

impl Client {
    pub(crate) async fn saved_messages(&self, identity: String, device: String) -> R<SpaceSummary> {
        let space = {
            let mut engine = self.lock();
            let space = engine.saved_messages_id(&identity, &device)?;
            if engine.saved_messages_canonical(&space) {
                return engine.saved_messages_ready(&space);
            }
            space
        };
        // Capture the existing owner AND exact authenticated channel before any
        // bootstrap effect; first-open offline cannot invent an accepted room.
        let (owner, channel) = self.device_directory_owner()?;
        let proposal = self.with_device_directory_completion(&owner, &channel, |engine| {
            engine.prepare_saved_messages(&identity, &device)
        })?;
        let Some(proposal) = proposal else {
            return self.with_device_directory_completion(&owner, &channel, |engine| {
                engine.saved_messages_ready(&space)
            });
        };
        let reply = channel
            .clone()
            .send(Op::EnsureSavedMessages(proposal))
            .await
            .map_err(|reason| refused(&reason))?;
        let Reply::SavedMessages(receipt) = reply else {
            return Err(refused("saved messages unsupported by this relay"));
        };
        let event = receipt
            .sequenced()
            .ok_or_else(|| refused("invalid saved messages receipt"))?;
        if event.env.space() != space || event.env.author() != identity {
            return Err(refused("foreign saved messages receipt"));
        }
        let result = self.with_device_directory_completion(&owner, &channel, |engine| {
            engine.native_surface_owner(&identity, &device)?;
            match engine.ingest(event) {
                crate::sync::Ingest::Applied
                | crate::sync::Ingest::Confirmed
                | crate::sync::Ingest::Duplicate => engine.saved_messages_ready(&space),
                _ => Err(refused("saved messages genesis was not admitted")),
            }
        });
        self.with_net(crate::net::Net::flush);
        result
    }
}

#[cfg(test)]
mod tests;
