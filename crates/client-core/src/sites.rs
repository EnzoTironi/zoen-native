//! Sites are ordinary encrypted `FileDoc` versions. Draft UI text never enters a
//! publication: review and explicit preparation re-read verified saved bytes.
use crate::{
    engine::{Engine, R},
    Client, CoreError, EntryKind,
};
use protocol::sites::{
    Change, Configuration, Inspection, Mutation, SignedMutation, SourceVersion, MAX_HTML_BYTES,
    SCHEMA,
};
use serde::{Deserialize, Serialize};
use types::{IdentityKind, ItemKind, Privacy};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub(crate) struct SiteScope {
    pub identity_id: String,
    pub device_id: String,
    pub space_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct SiteSource {
    pub source: SourceVersion,
    pub name: String,
    pub current_version: u32,
    pub editable: bool,
    /// Publishing an old saved version is allowed for current writers even
    /// though editing that old base is forbidden.
    pub publish_allowed: bool,
    /// None means the selected version needs downloading, never an empty draft.
    pub html: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub(crate) struct SiteReview {
    pub scope: SiteScope,
    pub source: SourceVersion,
    pub relay_url: String,
    pub configuration: Configuration,
    pub mutation: Mutation,
}

#[derive(Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct SitePrepared {
    pub relay_url: String,
    pub configuration: Configuration,
    pub signed: SignedMutation,
    pub html: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(crate) struct SiteStatus {
    pub configuration: Configuration,
    pub inspection: Inspection,
    pub pending: Option<SiteReview>,
}

fn invalid() -> CoreError {
    CoreError::Invalid {
        reason: crate::i18n::t(
            "Selecione uma versão HTML salva de até 2 MiB.",
            "Choose a saved HTML version up to 2 MiB.",
        ),
    }
}
fn forbidden() -> CoreError {
    CoreError::Forbidden {
        reason: crate::i18n::t(
            "O acesso a este site mudou.",
            "Access to this site changed.",
        ),
    }
}
fn stale() -> CoreError {
    CoreError::Stale {
        reason: crate::i18n::t(
            "A versão ou o destino mudou. Revise novamente.",
            "The version or destination changed. Review again.",
        ),
    }
}
fn html_valid(html: &str) -> R<()> {
    if html.is_empty()
        || u64::try_from(html.len()).map_err(|_| invalid())? > MAX_HTML_BYTES
        || html.contains('\0')
    {
        return Err(invalid());
    }
    Ok(())
}
fn hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn secret_id(s: &str, prefix: &str) -> bool {
    s.strip_prefix(prefix).is_some_and(|v| {
        v.len() == 32
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

fn valid_source(source: &SourceVersion) -> bool {
    let id = |value: &str| {
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    };
    source.version > 0
        && hex64(&source.sha256)
        && (1..=MAX_HTML_BYTES).contains(&source.byte_length)
        && id(&source.space_id)
        && id(&source.item_id)
}

fn safe_time() -> R<i64> {
    let now = crate::engine::now_ms();
    if !(1..=9_007_199_254_740_991).contains(&now) {
        return Err(invalid());
    }
    Ok(now)
}

impl Engine {
    fn site_scope(&self, scope: &SiteScope, write: bool) -> R<()> {
        self.native_surface_owner(&scope.identity_id, &scope.device_id)?;
        let space = self.space_state(&scope.space_id)?;
        if self.me_id()? != scope.identity_id
            || !self.account().is_some_and(|a| a.registered)
            || self.provider_setup_pending()
            || self
                .identities
                .get(&scope.identity_id)
                .is_none_or(|p| p.kind != IdentityKind::Person)
            || !self.is_member(&scope.space_id, &scope.identity_id)
            || space.privacy != Privacy::EndToEnd
            || space.integrity_error.is_some()
            || self.broken_spaces.contains_key(&scope.space_id)
            || (write && !self.state.writer(&scope.space_id, &scope.identity_id))
        {
            return Err(forbidden());
        }
        Ok(())
    }

    pub(crate) fn site_source(
        &self,
        scope: &SiteScope,
        item: &str,
        version: u32,
        sha: &str,
    ) -> R<SiteSource> {
        self.site_scope(scope, false)?;
        let it = self.item_state(item)?;
        if it.space != scope.space_id
            || it.kind != ItemKind::File
            || self.state.is_recovery_item(item)
        {
            return Err(forbidden());
        }
        let file = self.file_info_at(item, Some(version))?;
        if file.mime != "text/html" || !(1..=MAX_HTML_BYTES).contains(&file.bytes) || !hex64(sha) {
            return Err(invalid());
        }
        if file.sha256 != sha {
            return Err(stale());
        }
        let html = self
            .file_bytes_bounded(item, Some(version), MAX_HTML_BYTES)?
            .map(|bytes| String::from_utf8(bytes).map_err(|_| invalid()))
            .transpose()?;
        if let Some(html) = &html {
            html_valid(html)?;
        }
        let source = SourceVersion {
            space_id: scope.space_id.clone(),
            item_id: item.into(),
            version,
            sha256: sha.into(),
            byte_length: file.bytes,
        };
        if !valid_source(&source) {
            return Err(invalid());
        }
        Ok(SiteSource {
            source,
            name: file.name,
            current_version: it.current().number,
            editable: self.file_edit_allowed(item, version, sha)?,
            publish_allowed: self.state.writer(&scope.space_id, &scope.identity_id),
            html,
        })
    }

    pub(crate) fn site_create(
        &mut self,
        scope: &SiteScope,
        name: &str,
        html: &str,
        mutation_id: &str,
    ) -> R<SiteSource> {
        self.site_scope(scope, true)?;
        html_valid(html)?;
        if name.trim().is_empty()
            || name.len() > 255
            || !name.ends_with(".html")
            || name.contains(['/', '\\', '\0'])
        {
            return Err(invalid());
        }
        let entry = self.send_file_message(
            &scope.space_id,
            name,
            "text/html",
            html.as_bytes(),
            None,
            None,
            false,
            mutation_id,
        )?;
        let EntryKind::Message {
            card: Some(card), ..
        } = entry.kind
        else {
            return Err(invalid());
        };
        let file = self.file_info_at(&card.item_id, Some(card.version))?;
        self.site_source(scope, &card.item_id, card.version, &file.sha256)
    }

    pub(crate) fn site_save(
        &mut self,
        scope: &SiteScope,
        source: &SourceVersion,
        html: &str,
    ) -> R<SiteSource> {
        self.site_scope(scope, true)?;
        html_valid(html)?;
        let prior = self.site_source(scope, &source.item_id, source.version, &source.sha256)?;
        if prior.source != *source {
            return Err(stale());
        }
        self.file_new_version_checked(
            &source.item_id,
            source.version,
            &source.sha256,
            html.as_bytes(),
            None,
            "",
        )?;
        let item = self.item_state(&source.item_id)?;
        let version = item.current().number;
        let file = self.file_info_at(&source.item_id, Some(version))?;
        self.site_source(scope, &source.item_id, version, &file.sha256)
    }

    fn site_prepare(
        &self,
        review: &SiteReview,
        relay: &str,
        config: &Configuration,
    ) -> R<SitePrepared> {
        self.site_scope(&review.scope, true)?;
        if review.relay_url != relay
            || review.configuration != *config
            || review.mutation.public_origin != config.public_origin
        {
            return Err(stale());
        }
        let mutation = &review.mutation;
        if mutation.schema != SCHEMA
            || mutation.site_id
                != protocol::sites::site_id(
                    &review.scope.identity_id,
                    &review.scope.space_id,
                    &review.source.item_id,
                    &config.public_origin,
                )
                .map_err(|_| invalid())?
            || !secret_id(&mutation.request_id, "pub_")
            || mutation.expected_head.as_deref().is_some_and(|v| !hex64(v))
        {
            return Err(invalid());
        }
        let selected = self.site_source(
            &review.scope,
            &review.source.item_id,
            review.source.version,
            &review.source.sha256,
        )?;
        if selected.source != review.source {
            return Err(stale());
        }
        let html = match &mutation.change {
            Change::Publish { source } => {
                if selected.source != *source {
                    return Err(stale());
                }
                Some(selected.html.ok_or_else(|| CoreError::Invalid {
                    reason: crate::i18n::t(
                        "Baixe esta versão antes de publicar.",
                        "Download this version before publishing.",
                    ),
                })?)
            }
            Change::Unpublish => {
                if mutation.expected_head.is_none() {
                    return Err(invalid());
                }
                None
            }
        };
        let author = self.net.author.as_ref().ok_or_else(forbidden)?;
        let mut signed = SignedMutation {
            owner_id: review.scope.identity_id.clone(),
            device_id: review.scope.device_id.clone(),
            at_ms: safe_time()?,
            mutation: mutation.clone(),
            signature: String::new(),
        };
        let signing_bytes = signed
            .signing_bytes(&config.audience)
            .map_err(|_| invalid())?;
        signed.signature = author.key.sign(&signing_bytes);
        Ok(SitePrepared {
            relay_url: relay.into(),
            configuration: config.clone(),
            signed,
            html,
        })
    }
}

impl Client {
    fn site_destination(&self, scope: &SiteScope) -> R<(String, String)> {
        let relay = {
            let e = self.lock();
            e.site_scope(scope, true)?;
            e.account().ok_or_else(forbidden)?.relay_url.clone()
        };
        let audience = self
            .net
            .lock()
            .map_err(|_| CoreError::Invalid {
                reason: crate::i18n::t(
                    "A conexão precisa ser restabelecida antes de publicar.",
                    "Reconnect before publishing.",
                ),
            })?
            .as_ref()
            .and_then(|net| {
                net.authenticated_relay_name(&relay, &scope.identity_id, &scope.device_id)
            })
            .ok_or_else(|| CoreError::Invalid {
                reason: crate::i18n::t(
                    "Conecte-se antes de revisar a publicação.",
                    "Connect before reviewing publication.",
                ),
            })?;
        Ok((relay, audience))
    }
}

mod journal;
mod network;

#[cfg(test)]
mod tests;
