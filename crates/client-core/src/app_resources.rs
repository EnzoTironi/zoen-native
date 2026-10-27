//! Owned, signed MCP UI packages on the existing App/FileDoc authority.
use crate::engine::{Engine, State};
use crate::{AppResourceDto, CoreError, ItemDetail};
use serde::{Deserialize, Serialize};
use serde_json::json;
use types::{
    AppDoc, AppItemOrigin, Capability, EventBody, Grant, GrantScope, ItemContent, ItemKind,
    TrustLevel,
};

mod install;
#[cfg(test)]
mod tests;
mod update;
pub(crate) const MIME: &str = "text/html;profile=mcp-app";
pub(crate) const MAX_BYTES: u64 = 8 * 1024 * 1024;
const PREFIX: &str = "ui://zoen/package/";
type R<T> = Result<T, CoreError>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct Install {
    pub expected_identity_id: String,
    pub expected_device_id: String,
    pub space_id: String,
    pub package_item_id: String,
    pub package_version: u32,
    pub package_sha256: String,
    pub title: String,
    pub mutation_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct Update {
    pub admission: Install,
    pub item_id: String,
    pub expected_version: u32,
    pub expected_sha256: String,
    pub expected_resource_uri: String,
}
fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: crate::i18n::t("Pacote de App inválido.", "Invalid App package."),
    }
}
fn denied() -> CoreError {
    CoreError::Forbidden {
        reason: crate::i18n::t("O acesso a este App mudou.", "Access to this App changed."),
    }
}
fn token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn hash(bytes: &[u8]) -> String {
    event_log::sha256_hex(bytes)
}
pub(crate) fn item_id(owner: &str, device: &str, space: &str, mutation: &str) -> R<String> {
    Ok(format!(
        "it_mcp_{}",
        hash(
            &serde_json::to_vec(&("zoen-owned-mcp-app-v1", owner, device, space, mutation))
                .map_err(|_| invalid())?
        )
    ))
}
fn package_uri(input: &Install) -> R<String> {
    if !token(&input.package_item_id)
        || input.package_version == 0
        || input.package_sha256.len() != 64
        || !input
            .package_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    Ok(format!(
        "{PREFIX}{}/{}/{}",
        input.package_item_id, input.package_version, input.package_sha256
    ))
}
pub(crate) fn valid(app: &AppDoc) -> bool {
    let Some(view) = &app.view else {
        return !app.resource_uri.starts_with(PREFIX);
    };
    let Some(rest) = app.resource_uri.strip_prefix(PREFIX) else {
        return false;
    };
    let parts: Vec<_> = rest.split('/').collect();
    app.app == "excalidraw"
        && parts.len() == 3
        && token(parts[0])
        && parts[1]
            .parse::<u32>()
            .is_ok_and(|n| n > 0 && n.to_string() == parts[1])
        && parts[2] == view.sha256
        && view.mime == MIME
        && view.bytes > 0
        && view.path.is_empty()
        && view.voice.is_none()
        && view.thumb.is_none()
        && view.app_origin.is_none()
        && crate::files::validate_file(view, MAX_BYTES).is_ok()
}
pub(crate) fn event_valid(
    state: &State,
    event: &types::Event,
    identities: &std::collections::HashMap<String, types::Identity>,
) -> bool {
    let (item, next) = match &event.body {
        EventBody::ItemCreated {
            item,
            content: ItemContent::App(app),
            ..
        }
        | EventBody::ItemVersioned {
            item,
            content: ItemContent::App(app),
            ..
        } => (item, app),
        EventBody::ItemReverted {
            item, to_version, ..
        } => {
            let Some(target) = state
                .items
                .get(item)
                .and_then(|it| it.versions.iter().find(|v| v.number == *to_version))
            else {
                return true;
            };
            let ItemContent::App(app) = &target.content else {
                return !state.items.get(item).is_some_and(
                    |it| matches!(&it.current().content, ItemContent::App(a) if a.view.is_some()),
                );
            };
            (item, app)
        }
        EventBody::ItemVersioned { item, .. } => {
            return !state.items.get(item).is_some_and(
                |it| matches!(&it.current().content, ItemContent::App(a) if a.view.is_some()),
            );
        }
        EventBody::AppCurrentShared { share } => return valid(&share.content),
        _ => return true,
    };
    if !valid(next) {
        return false;
    }
    if next.view.is_some() {
        match &event.body {
            EventBody::ItemCreated { kind, .. } if *kind != ItemKind::App => return false,
            EventBody::ItemVersioned { .. } | EventBody::ItemReverted { .. }
                if !state.items.get(item).is_some_and(|it| {
                    it.kind == ItemKind::App
                        && matches!(&it.versions.initial().content,
                            ItemContent::App(app) if app.view.is_some())
                }) =>
            {
                return false;
            }
            _ => {}
        }
        if !state.writer(&event.space, &event.author) {
            return false;
        }
        let Some(actor) = identities.get(&event.author) else {
            return false;
        };
        if actor.kind == types::IdentityKind::Agent
            && (matches!(&event.body, EventBody::ItemCreated { .. })
                || (!actor
                    .owner
                    .as_deref()
                    .and_then(|owner| state.latest_trust(&event.space, owner, &event.author))
                    .is_some_and(|g| {
                        g.grant.capability == Capability::Trust(TrustLevel::Act)
                            && g.grant.expires_at_ms.is_none_or(|at| at > event.at_ms)
                    })
                    && !actor.owner.as_deref().is_some_and(|owner| {
                        crate::agent_scenes::approved_effect(state, event, owner)
                    })))
        {
            return false;
        }
        if matches!(&event.body, EventBody::ItemCreated { .. })
            && !next
                .document
                .as_ref()
                .and_then(|f| f.app_origin.as_ref())
                .is_some_and(|origin| {
                    origin.author == event.author
                        && event.device.as_deref() == Some(origin.device.as_str())
                        && origin.space == event.space
                        && token(&origin.mutation)
                        && item_id(
                            &origin.author,
                            &origin.device,
                            &origin.space,
                            &origin.mutation,
                        )
                        .is_ok_and(|id| id == *item)
                })
        {
            return false;
        }
    }
    if let Some(prior) = state.items.get(item) {
        if let ItemContent::App(before) = &prior.current().content {
            if before.view.is_some() != next.view.is_some() {
                return false;
            }
            // Package updates never silently replace the renderer of an open artifact.
            if before.view.is_some()
                && (before.view != next.view || before.resource_uri != next.resource_uri)
            {
                let owner = &prior.versions.initial().author;
                let permission = state.app_trust_baseline(item);
                if next.view.is_none()
                    || before.document != next.document
                    || before.title != next.title
                    || before.state_json != next.state_json
                    || &event.author != owner
                    || !state.writer(&event.space, owner)
                    || !permission.is_some_and(|g| {
                        !g.revoked
                            && g.grant.grantor == *owner
                            && g.grant.capability == Capability::Trust(TrustLevel::Act)
                            && g.grant.expires_at_ms.is_none_or(|at| at > event.at_ms)
                    })
                {
                    return false;
                }
            }
        }
    }
    true
}
impl Engine {
    pub(crate) fn check_app_resource_pin(&self, item: &str, expected: &str) -> R<()> {
        let (_, _, _, _, app) = self.app_document_scope(item)?;
        if app.resource_uri != expected {
            return Err(CoreError::Stale {
                reason: crate::i18n::t(
                    "O App mudou. Sua edição foi mantida.",
                    "The App changed. Your edit has been preserved.",
                ),
            });
        }
        if app.view.is_some()
            && (self.app_trust(item) < TrustLevel::Act || !self.app_trust_available(item))
        {
            return Err(denied());
        }
        Ok(())
    }
    pub(crate) fn app_document_resource(&self, item: &str, expected: &str) -> R<AppResourceDto> {
        self.check_app_resource_pin(item, expected)?;
        let (_, _, _, _, app) = self.app_document_scope(item)?;
        let Some(view) = &app.view else {
            return self.read_app_resource(&app.resource_uri);
        };
        if !valid(&app) || self.app_trust(item) < TrustLevel::Act || !self.app_trust_available(item)
        {
            return Err(denied());
        }
        let bytes = self
            .owned_file_bytes(view, MAX_BYTES)?
            .ok_or_else(|| CoreError::Invalid {
                reason: crate::i18n::t(
                    "O pacote do App ainda está chegando.",
                    "The App package is still arriving.",
                ),
            })?;
        let html = String::from_utf8(bytes).map_err(|_| invalid())?;
        Ok(AppResourceDto {
            uri: app.resource_uri.clone(),
            mime_type: MIME.into(),
            text: html,
            prefers_border: false,
            manifest_json:
                json!({"id":"excalidraw","name":app.title,"sha256":view.sha256,"bytes":view.bytes,
                "capabilities":[],"allowedDomains":[],"linkDomains":[]})
                .to_string(),
        })
    }
    pub(crate) fn copy_app_view(&mut self, app: &mut AppDoc, space: &str, key: &[u8; 32]) -> R<()> {
        if let Some(view) = &app.view {
            let bytes = self
                .owned_file_bytes(view, MAX_BYTES)?
                .ok_or_else(invalid)?;
            app.view = Some(Box::new(self.file_doc(
                space,
                key,
                "app.html",
                "",
                MIME,
                &bytes,
                None,
                &std::collections::HashMap::new(),
            )?));
        }
        Ok(())
    }
}
