use super::*;
use crate::pages::{BeforeEvent, Projection};
use std::collections::HashMap;
use types::{Capability, GrantScope, Identity, ItemContent, ItemKind, TrustLevel};

impl State {
    pub(crate) fn app_discussion_anchor(&self, space: &str, item: &str) -> Option<&str> {
        let state = self.items.get(item).filter(|it| it.space == space)?;
        let entries = &self.spaces.get(space)?.entries;
        if let Some(entry) = entries.iter().find(|entry| {
            matches!(&entry.body,
            EntryBody::Message { attaches: Some(id), reply: None, marks, .. }
                if id == item && !marks.forwarded)
        }) {
            return matches!(&entry.body, EntryBody::Message { marks, .. } if !marks.deleted)
                .then(|| entry.id());
        }
        state
            .shared
            .as_ref()
            .map(|origin| origin.anchor.event.as_str())
    }

    pub(crate) fn apply_app_current_share(
        &mut self,
        event: &Event,
        share: &AppCurrentShare,
        identities: &HashMap<String, Identity>,
        context: &BeforeEvent,
    ) -> Projection {
        if !context.app_share_refs_valid()
            || share.schema != 1
            || !token(&share.item)
            || !token(&share.mutation)
            || !token(&share.grant.id)
            || share.version == 0
            || !valid_document(&share.content)
            || !self.writer(&event.space, &event.author)
            || !self.spaces.get(&event.space).is_some_and(|space| {
                space.members.iter().any(|(id, _)| id == &share.target)
                    && (space.privacy == types::Privacy::EndToEnd) == share.epoch.is_some()
            })
            || !identities
                .get(&event.author)
                .is_some_and(|id| id.kind == types::IdentityKind::Person)
            || !self.grant_issuer_allowed(event, &share.grant, identities)
            || share.grant.scope != GrantScope::Item(share.item.clone())
            || share.grant.grantee.as_deref() != Some(format!("app:{}", share.item).as_str())
            || share.grant.capability != Capability::Trust(TrustLevel::Act)
            || share
                .grant
                .expires_at_ms
                .is_some_and(|at| at <= event.at_ms)
            || (context.ordered()
                && (share.cut.seq.checked_add(1) != Some(event.seq)
                    || share.cut.hash != event.prev))
        {
            return Projection::Refused;
        }

        if let Some(known) = self.items.get(&share.item) {
            if known.space != event.space
                || known.kind != ItemKind::App
                || known.current().number > share.version
                || (known.current().number == share.version
                    && known.current().content != ItemContent::App(share.content.clone()))
                || self.app_discussion_anchor(&event.space, &share.item)
                    != Some(share.anchor.event.as_str())
                || known
                    .shared
                    .as_ref()
                    .is_some_and(|origin| origin.creation != share.creation)
                || (known.shared.is_none() && known.current().number != share.version)
            {
                return Projection::Refused;
            }
            // A known revoked/expired capability cannot be revived by sharing.
            if self.app_trust_baseline(&share.item).is_none_or(|grant| {
                grant.revoked || grant.grant.expires_at_ms.is_some_and(|at| at <= event.at_ms)
                    || grant.grant.expires_at_ms != share.grant.expires_at_ms
                    || !matches!(grant.grant.capability, Capability::Trust(level) if level >= TrustLevel::Act)
            }) { return Projection::Refused; }
        }
        let version = Version {
            number: share.version,
            content: ItemContent::App(share.content.clone()),
            author: event.author.clone(),
            at_ms: event.at_ms,
            note: t("Versão atual compartilhada", "Current version shared"),
            is_undo: false,
        };
        let origin = SharedOrigin {
            creation: share.creation.clone(),
            head: share.head.clone(),
            anchor: share.anchor.clone(),
            event: event.client_id.clone(),
        };
        match self.items.get_mut(&share.item) {
            Some(known) if known.shared.is_some() => {
                if known.current().number < share.version {
                    known.versions.push(version);
                }
                known.shared = Some(origin);
            }
            Some(_) => {} // Full known history and its authorship remain intact.
            None => {
                self.items.insert(
                    share.item.clone(),
                    ItemState {
                        id: share.item.clone(),
                        space: event.space.clone(),
                        kind: ItemKind::App,
                        origin: t("Versão atual compartilhada", "Current version shared"),
                        versions: vec![version],
                        shared: Some(origin),
                        has_voice_history: false,
                    },
                );
            }
        }
        self.apply_issued_grant(event, &share.grant);
        if let Some(space) = self.spaces.get_mut(&event.space) {
            space.entries.push(Entry {
                hash: event.hash.clone(),
                client_id: event.client_id.clone(),
                space: event.space.clone(),
                seq: event.seq,
                author: event.author.clone(),
                at_ms: event.at_ms,
                body: EntryBody::System {
                    text: t(
                        "App compartilhado nesta conversa",
                        "App shared in this conversation",
                    ),
                },
            });
            space.last_at_ms = space.last_at_ms.max(event.at_ms);
        }
        Projection::Applied
    }
}

impl Engine {
    pub(crate) fn shared_app_root(&self, space: &str, item: &str) -> Option<TimelineEntry> {
        let state = self.state.items.get(item).filter(|it| it.space == space)?;
        let shared = state.shared.as_ref()?;
        if self.state.app_discussion_anchor(space, item) != Some(shared.anchor.event.as_str()) {
            return None;
        }
        let entry = self
            .state
            .spaces
            .get(space)?
            .entries
            .iter()
            .find(|entry| entry.id() == shared.event)?;
        let mut root = self.entry_dto(entry);
        // A view of the sharing attestation using the immutable discussion anchor,
        // never a fabricated original plaintext message or historical signature.
        root.id = shared.anchor.event.clone();
        root.seq = shared.anchor.seq;
        Some(root)
    }

    pub(crate) fn shared_app_root_by_anchor(
        &self,
        space: &str,
        anchor: &str,
    ) -> Option<TimelineEntry> {
        let item = self.state.items.values().find(|item| {
            item.space == space
                && item
                    .shared
                    .as_ref()
                    .is_some_and(|origin| origin.anchor.event == anchor)
        })?;
        self.shared_app_root(space, &item.id)
    }
}
