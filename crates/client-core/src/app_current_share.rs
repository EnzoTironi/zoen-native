//! One explicit signed current-state disclosure; no history import or new journal.
use crate::{engine::*, i18n::t, Client, CoreError, TimelineEntry};
use serde::{Deserialize, Serialize};
use types::{AppCurrentShare, AppDoc, AppShareRef, Event, EventBody};

mod origin;
mod producer;
mod projection;
mod transport;
pub(crate) use transport::request_share_mutation;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct AppShareScope {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub item_id: String,
    pub target_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct AppShareContext {
    pub scope: AppShareScope,
    pub version: u32,
    pub sha256: String,
    /// Binds the observed epoch, canonical cut, capability and exact document.
    pub revision: String,
}

#[derive(Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct AppShareReceipt {
    pub entry: TimelineEntry,
    pub item_id: String,
    pub target_id: String,
    pub version: u32,
    pub sha256: String,
    pub applied: bool,
}

#[derive(Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct AppShareResult {
    pub receipt: Option<AppShareReceipt>,
    pub conflict: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct SharedOrigin {
    pub creation: AppShareRef,
    pub head: AppShareRef,
    pub anchor: AppShareRef,
    pub event: String,
}

#[derive(Clone)]
pub(crate) struct SharedVersionInfo {
    pub author: String,
    pub at_ms: i64,
    pub note: String,
    pub is_undo: bool,
}

pub(crate) fn readable_head(prefix: &[Event], event: &Event) -> Option<SharedVersionInfo> {
    let EventBody::AppCurrentShared { share } = &event.body else {
        return None;
    };
    let head = usize::try_from(share.head.seq)
        .ok()
        .and_then(|n| prefix.get(n))?;
    let (note, is_undo) = match &head.body {
        EventBody::ItemVersioned {
            item,
            content: types::ItemContent::App(app),
            note,
        } if item == &share.item && origin::same_document(app, &share.content) => {
            (note.clone(), false)
        }
        EventBody::ItemCreated {
            item,
            content: types::ItemContent::App(app),
            ..
        } if item == &share.item && origin::same_document(app, &share.content) => {
            (t("Criado", "Created"), false)
        }
        EventBody::ItemReverted { item, note, .. } if item == &share.item => (note.clone(), true),
        _ => return None,
    };
    Some(SharedVersionInfo {
        author: head.author.clone(),
        at_ms: head.at_ms,
        note,
        is_undo,
    })
}

/// Chat-only progress may follow the signed observation. Unknown or policy
/// effects require a fresh review; none can silently widen the disclosure.
fn same_share_context(body: &EventBody, share: &AppCurrentShare) -> bool {
    match body {
        EventBody::MessagePosted { attaches, .. } => {
            attaches.as_deref() != Some(share.item.as_str())
        }
        EventBody::MessageAnnotated { target, .. } => target != &share.anchor.event,
        EventBody::Checkpoint { epoch, .. } => Some(*epoch) == share.epoch,
        EventBody::ProfileKeyShared { .. }
        | EventBody::ChatPrefsSet { .. }
        | EventBody::CallV1 { .. }
        | EventBody::CallTransportV1 { .. }
        | EventBody::AppEditRequested { .. } => true,
        EventBody::ItemCreated { item, .. }
        | EventBody::ItemVersioned { item, .. }
        | EventBody::ItemReverted { item, .. } => item != &share.item,
        EventBody::AppCurrentShared { share: other } => other.item != share.item,
        EventBody::GrantIssued { grant } => matches!(&grant.scope,
            types::GrantScope::Item(item) if item != &share.item),
        _ => false,
    }
}

fn current_prefix(prefix: &[Event], share: &AppCurrentShare) -> bool {
    let mut roles = std::collections::HashMap::new();
    for event in prefix {
        if event.seq > share.cut.seq && !same_share_context(&event.body, share) {
            return false;
        }
        let writer = roles
            .get(&event.author)
            .is_some_and(|r| *r != types::Role::Reader);
        if (event.seq == share.creation.seq || event.seq == share.head.seq) && !writer {
            return false;
        }
        if event.seq > share.head.seq
            && writer
            && matches!(&event.body,
            EventBody::ItemVersioned { item, .. } | EventBody::ItemReverted { item, .. } if item == &share.item)
        {
            return false;
        } // Refuse a stale snapshot, never discard an already retained successor.
        match &event.body {
            EventBody::SpaceCreated { .. } => {
                roles.insert(event.author.clone(), types::Role::Owner);
            }
            EventBody::MemberAdded { identity, role } => {
                roles.insert(identity.clone(), *role);
            }
            EventBody::MemberRemoved { identity } => {
                roles.remove(identity);
            }
            _ => {}
        }
    }
    true
}

pub(crate) fn refused() -> CoreError {
    CoreError::Stale {
        reason: t(
            "O App, o acesso ou a conversa mudou. Confira antes de compartilhar.",
            "The App, access or conversation changed. Review before sharing.",
        ),
    }
}

fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn reference(event: &Event) -> AppShareRef {
    AppShareRef {
        event: event.client_id.clone(),
        seq: event.seq,
        hash: event.hash.clone(),
        author: event.author.clone(),
        device: event.device.clone(),
        at_ms: event.at_ms,
    }
}

fn valid_document(app: &AppDoc) -> bool {
    !app.title.trim().is_empty()
        && app.title.chars().count() <= 160
        && app.state_json.len() <= 128 * 1024
        && serde_json::from_str::<serde_json::Value>(&app.state_json).is_ok()
        && miniapps::binary::Format::for_id(&app.app).is_some_and(|format| {
            app.resource_uri == format.resource()
                && app.document.as_ref().is_some_and(|file| {
                    file.mime == format.mime()
                        && file.voice.is_none()
                        && file.path.is_empty()
                        && crate::files::validate_file(file, format.max_bytes() as u64).is_ok()
                })
        })
}

/// Checked against authenticated prefix headers. Opaque historical applications
/// stay opaque; these references do not assert an original plaintext opening.
pub(crate) fn references_valid(prefix: &[Event], event: &Event) -> bool {
    let EventBody::AppCurrentShared { share } = &event.body else {
        return true;
    };
    let at = |r: &AppShareRef| usize::try_from(r.seq).ok().and_then(|n| prefix.get(n));
    let matches = |r: &AppShareRef| at(r).is_some_and(|e| reference(e) == *r);
    let opaque = |e: &Event| {
        matches!(&e.body, EventBody::Sealed { kind }
        if kind == event_log::content::SealedKind::Application.name())
    };
    event.seen.as_ref() == Some(&share.cut)
        && origin::valid(&share.content, &share.creation, &event.space, &share.item)
        && current_prefix(prefix, share)
        && usize::try_from(share.cut.seq)
            .ok()
            .and_then(|seq| prefix.get(seq))
            .is_some_and(|e| e.seq == share.cut.seq && e.hash == share.cut.hash)
        && [&share.creation, &share.head, &share.anchor]
            .into_iter()
            .all(matches)
        && share.creation.seq <= share.head.seq
        && share.head.seq <= share.cut.seq
        && share.creation.seq <= share.anchor.seq
        && share.anchor.seq <= share.cut.seq
        && at(&share.creation).is_some_and(|e| {
            opaque(e)
                || matches!(&e.body, EventBody::ItemCreated { item, .. } if item == &share.item)
        })
        && at(&share.head).is_some_and(|e| {
            opaque(e)
                || matches!(&e.body,
            EventBody::ItemCreated { item, .. } | EventBody::ItemVersioned { item, .. }
                | EventBody::ItemReverted { item, .. } if item == &share.item)
        })
        && at(&share.anchor).is_some_and(|e| {
            opaque(e)
                || matches!(&e.body,
            EventBody::MessagePosted { attaches: Some(item), reply: None, forwarded: false, .. }
                if item == &share.item)
        })
        && !prefix.iter().any(|e| {
            e.author == event.author
                && matches!(&e.body,
            EventBody::AppCurrentShared { share: prior } if prior.mutation == share.mutation)
        })
}

impl Client {
    pub(crate) fn app_current_share_context(&self, scope: AppShareScope) -> R<AppShareContext> {
        self.lock().app_current_share_context(scope)
    }
    pub(crate) fn share_app_current(
        &self,
        context: AppShareContext,
        mutation: &str,
    ) -> R<AppShareReceipt> {
        self.lock().share_app_current(context, mutation)
    }
}
