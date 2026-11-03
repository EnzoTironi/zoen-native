//! Materialize an item only after an owner resolution and its receipt were validated.
use super::super::{
    documents, Capability, Event, EventBody, Grant, GrantRead, GrantScope, ItemKind, Projection,
    RequestRead, TrustLevel,
};

pub(super) fn apply_item(
    state: &mut Projection,
    space: &str,
    event: &Event,
    pending: bool,
    original: &RequestRead,
) {
    let Some(proposal) = &original.request.proposal else {
        return;
    };
    let mut virtual_event = event.clone();
    virtual_event.author.clone_from(&original.request.agent);
    virtual_event.body = EventBody::ItemCreated {
        item: proposal.item.clone(),
        kind: proposal.kind,
        content: proposal.content.clone(),
        origin: proposal.origin.clone(),
    };
    state.virtual_items.insert(
        (space.into(), event.client_id.clone()),
        virtual_event.clone(),
    );
    state.items.insert(
        proposal.item.clone(),
        documents::Item {
            id: proposal.item.clone(),
            space: space.into(),
            kind: proposal.kind,
            origin: proposal.origin.clone(),
            versions: documents::VersionHistory::new(documents::Version {
                content: proposal.content.clone(),
                event: virtual_event,
                note: "Criado".into(),
                is_undo: false,
                pending,
                failure: None,
            }),
        },
    );
    if proposal.kind == ItemKind::App {
        state.grants.push(GrantRead {
            grant: Grant {
                id: format!("gr_app_{}", original.request.id),
                grantor: event.author.clone(),
                grantee: Some(format!("app:{}", proposal.item)),
                scope: GrantScope::Item(proposal.item.clone()),
                capability: Capability::Trust(TrustLevel::Act),
                expires_at_ms: None,
            },
            space: space.into(),
            at: event.at_ms,
            revoked: false,
        });
    }
}
