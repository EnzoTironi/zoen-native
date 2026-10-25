use super::*;
use types::{Capability, Grant, GrantScope, IdentityKind, ItemContent, Seen, TrustLevel};

impl Engine {
    fn share_owner(&self, scope: &AppShareScope) -> R<()> {
        self.native_surface_owner(&scope.expected_identity_id, &scope.expected_device_id)?;
        if !self
            .state
            .writer(&scope.space_id, &scope.expected_identity_id)
            || !self
                .identities
                .get(&scope.expected_identity_id)
                .is_some_and(|i| i.kind == IdentityKind::Person)
        {
            return Err(refused());
        }
        Ok(())
    }

    fn share_snapshot(&self, scope: &AppShareScope) -> R<AppCurrentShare> {
        self.share_owner(scope)?;
        if !self.is_member(&scope.space_id, &scope.target_id)
            || self.checked_integrity_error(&scope.space_id)?.is_some()
            || self
                .store
                .outbox()?
                .iter()
                .any(|row| !row.failed && row.event.space == scope.space_id)
            || self.app_trust(&scope.item_id) < TrustLevel::Act
            || !self.app_trust_available(&scope.item_id)
        {
            return Err(refused());
        }
        let epoch = if self.is_e2e(&scope.space_id) {
            let ready = self.ready_epoch(&scope.space_id).ok_or_else(refused)?;
            if !self
                .mls_status(&scope.space_id)
                .is_some_and(|(_, _, members)| members.contains(&scope.target_id))
            {
                return Err(CoreError::Stale {
                    reason: t(
                        "Aguarde a entrada do participante na conversa.",
                        "Wait for the participant to join this conversation.",
                    ),
                });
            }
            Some(ready)
        } else {
            None
        };
        let item = self.item_state(&scope.item_id)?;
        let current = item.current();
        let ItemContent::App(content) = &current.content else {
            return Err(refused());
        };
        if item.space != scope.space_id || !valid_document(content) {
            return Err(refused());
        }
        let file = content.document.as_ref().ok_or_else(refused)?;
        // Validate the actual owned bytes/codec before disclosure, not just counts.
        self.app_binary_read(&scope.item_id, Some(current.number), Some(&file.sha256))?;
        let log = self.logs.get(&scope.space_id).ok_or_else(refused)?;
        let creation = if let Some(origin) = &item.shared {
            origin.creation.clone()
        } else {
            log.events().iter().find(|e| matches!(&e.body, EventBody::ItemCreated { item, .. } if item == &scope.item_id))
                .map(reference).ok_or_else(refused)?
        };
        let head = log
            .events()
            .iter()
            .rev()
            .find(|e| {
                e.author == current.author
                    && e.at_ms == current.at_ms
                    && matches!(&e.body,
                EventBody::ItemCreated { item, .. } | EventBody::ItemVersioned { item, .. }
                    | EventBody::ItemReverted { item, .. } if item == &scope.item_id)
            })
            .map(reference)
            .or_else(|| item.shared.as_ref().map(|o| o.head.clone()))
            .ok_or_else(refused)?;
        let anchor = if let Some(origin) = &item.shared {
            origin.anchor.clone()
        } else {
            let root = self
                .item_discussion(&scope.space_id, &scope.item_id)?
                .root
                .filter(|r| !r.deleted)
                .ok_or_else(refused)?;
            log.events()
                .iter()
                .find(|e| e.client_id == root.id)
                .map(reference)
                .ok_or_else(refused)?
        };
        let cut: Seen = log.head().ok_or_else(refused)?;
        let permission = self
            .state
            .app_trust_baseline(&scope.item_id)
            .ok_or_else(refused)?;
        let mut content = content.clone();
        let origin = self.app_share_origin(&scope.item_id, &scope.space_id, &creation)?;
        content.document.as_mut().ok_or_else(refused)?.app_origin = Some(origin);
        Ok(AppCurrentShare {
            schema: 2,
            item: scope.item_id.clone(),
            target: scope.target_id.clone(),
            mutation: String::new(),
            version: current.number,
            content,
            creation,
            head,
            anchor,
            cut,
            epoch,
            grant: Grant {
                id: String::new(),
                grantor: scope.expected_identity_id.clone(),
                grantee: Some(format!("app:{}", scope.item_id)),
                scope: GrantScope::Item(scope.item_id.clone()),
                capability: Capability::Trust(TrustLevel::Act),
                expires_at_ms: permission.grant.expires_at_ms,
            },
        })
    }

    /// Resume a genuine prefix receipt before constructing any fresh snapshot.
    /// The outer request savepoint contains both this normal share and the reply.
    pub(crate) fn share_request_current(
        &mut self,
        scope: AppShareScope,
        version: u32,
        sha256: &str,
        mutation: &str,
    ) -> R<()> {
        self.share_owner(&scope)?;
        if self.app_trust(&scope.item_id) < TrustLevel::Act
            || !self.app_trust_available(&scope.item_id)
        {
            return Err(refused());
        }
        let mut events = self
            .logs
            .get(&scope.space_id)
            .map_or_else(Vec::new, |log| log.events().to_vec());
        events.extend(
            self.store
                .outbox()?
                .into_iter()
                .filter(|r| r.event.space == scope.space_id)
                .map(|r| r.event),
        );
        for event in events {
            let EventBody::AppCurrentShared { share } = &event.body else {
                continue;
            };
            if event.author != scope.expected_identity_id || share.mutation != mutation {
                continue;
            }
            if share.item != scope.item_id
                || share.target != scope.target_id
                || share.version != version
                || share
                    .content
                    .document
                    .as_ref()
                    .is_none_or(|file| file.sha256 != sha256)
                || !self.share_receipt(&event, share).applied
            {
                return Err(refused());
            }
            return Ok(());
        }
        let context = self.app_current_share_context(scope)?;
        if context.version != version || context.sha256 != sha256 {
            return Err(refused());
        }
        if !self.share_app_current(context, mutation)?.applied {
            return Err(refused());
        }
        Ok(())
    }

    pub(crate) fn app_current_share_context(&self, scope: AppShareScope) -> R<AppShareContext> {
        let snapshot = self.share_snapshot(&scope)?;
        let file = snapshot.content.document.as_ref().ok_or_else(refused)?;
        let revision = event_log::content_hash(&(&scope, &snapshot));
        Ok(AppShareContext {
            scope,
            version: snapshot.version,
            sha256: file.sha256.clone(),
            revision,
        })
    }

    fn share_receipt(&self, event: &Event, share: &AppCurrentShare) -> AppShareReceipt {
        let applied = self
            .state
            .spaces
            .get(&event.space)
            .is_some_and(|space| space.entries.iter().any(|e| e.id() == event.client_id));
        let entry = Entry {
            hash: event.hash.clone(),
            client_id: event.client_id.clone(),
            space: event.space.clone(),
            seq: event.seq,
            author: event.author.clone(),
            at_ms: event.at_ms,
            body: EntryBody::System {
                text: if applied {
                    t(
                        "App compartilhado nesta conversa",
                        "App shared in this conversation",
                    )
                } else {
                    t("Compartilhamento não aplicado", "Sharing was not applied")
                },
            },
        };
        AppShareReceipt {
            entry: self.entry_dto(&entry),
            item_id: share.item.clone(),
            target_id: share.target.clone(),
            version: share.version,
            sha256: share
                .content
                .document
                .as_ref()
                .map_or_else(String::new, |f| f.sha256.clone()),
            applied,
        }
    }

    pub(crate) fn share_app_current(
        &mut self,
        context: AppShareContext,
        mutation: &str,
    ) -> R<AppShareReceipt> {
        self.share_owner(&context.scope)?;
        if !token(mutation) {
            return Err(refused());
        }
        // The signed canonical event or its original outbox intent is the receipt.
        // A later revocation/head cannot republish or silently renew this operation.
        let mut events: Vec<Event> = self
            .logs
            .get(&context.scope.space_id)
            .map_or_else(Vec::new, |l| l.events().to_vec());
        events.extend(
            self.store
                .outbox()?
                .into_iter()
                .filter(|r| r.event.space == context.scope.space_id)
                .map(|r| r.event),
        );
        for event in events {
            let EventBody::AppCurrentShared { share } = &event.body else {
                continue;
            };
            if event.author == context.scope.expected_identity_id && share.mutation == mutation {
                let mut original = share.clone();
                original.mutation.clear();
                original.grant.id.clear();
                if share.item != context.scope.item_id
                    || share.target != context.scope.target_id
                    || share.version != context.version
                    || event_log::content_hash(&(&context.scope, &original)) != context.revision
                    || share
                        .content
                        .document
                        .as_ref()
                        .is_none_or(|f| f.sha256 != context.sha256)
                {
                    return Err(refused());
                }
                return Ok(self.share_receipt(&event, share));
            }
        }
        if self.app_current_share_context(context.scope.clone())? != context {
            return Err(refused());
        }
        let mut share = self.share_snapshot(&context.scope)?;
        share.mutation = mutation.into();
        share.grant.id = types::new_id("gr");
        self.atomic_agent_action(|engine| {
            let event = engine.append(
                &context.scope.space_id,
                &context.scope.expected_identity_id,
                EventBody::AppCurrentShared {
                    share: Box::new(share.clone()),
                },
            )?;
            Ok(engine.share_receipt(&event, &share))
        })
    }
}
