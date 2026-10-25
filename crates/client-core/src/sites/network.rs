//! Only the authenticated relay discovers publication destinations. All HTTP
//! clients are disposable, cookie-free, redirect-free and bounded.
use super::{
    forbidden, hex64, invalid, safe_time, stale, valid_source, Change, Client, Configuration,
    CoreError, Inspection, Mutation, Serialize, SignedMutation, SitePrepared, SiteReview,
    SiteScope, SiteStatus, SourceVersion, MAX_HTML_BYTES, R, SCHEMA,
};
use protocol::sites::{Receipt, SignedInspection};
use serde::de::DeserializeOwned;
use std::time::Duration;

fn unavailable() -> CoreError {
    CoreError::Invalid {
        reason: crate::i18n::t(
            "A publicação não está disponível. Tente novamente.",
            "Publication is unavailable. Try again.",
        ),
    }
}
fn http() -> R<reqwest::Client> {
    crate::https::client()?
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| unavailable())
}
fn origin(value: &str) -> R<reqwest::Url> {
    let url = reqwest::Url::parse(value).map_err(|_| invalid())?;
    let local = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if value.len() > 2048
        || !(url.scheme() == "https" || url.scheme() == "http" && local)
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.origin().ascii_serialization() != value
    {
        return Err(invalid());
    }
    Ok(url)
}
fn configuration(config: &Configuration, relay: &str, audience: &str) -> R<()> {
    let management = origin(&config.management_origin)?;
    let public = origin(&config.public_origin)?;
    let relay = reqwest::Url::parse(&crate::net::http_base(relay)).map_err(|_| invalid())?;
    if config.schema != SCHEMA
        || config.audience != audience
        || config.max_html_bytes != MAX_HTML_BYTES
        || management.host_str() == public.host_str()
        || relay.host_str() == public.host_str()
    {
        return Err(stale());
    }
    Ok(())
}
async fn json<T: DeserializeOwned>(mut response: reqwest::Response, maximum: usize) -> R<T> {
    let maximum_u64 = u64::try_from(maximum).map_err(|_| invalid())?;
    if !response.status().is_success() || response.content_length().is_some_and(|n| n > maximum_u64)
    {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if chunk.len() > maximum.saturating_sub(bytes.len()) {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| unavailable())
}

impl Client {
    async fn site_configuration(&self, scope: &SiteScope) -> R<Option<(String, Configuration)>> {
        let (relay, audience) = self.site_destination(scope)?;
        let base = crate::net::http_base(&relay);
        // A relay may carry an installation path; it cannot inject credentials,
        // queries, insecure remote HTTP or a different publication destination.
        let parsed = reqwest::Url::parse(&base).map_err(|_| invalid())?;
        origin(&parsed.origin().ascii_serialization())?;
        if !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(invalid());
        }
        let response = http()?
            .get(format!("{base}/v1/sites/config"))
            .send()
            .await
            .map_err(|_| unavailable())?;
        if self.site_destination(scope)? != (relay.clone(), audience.clone()) {
            return Err(stale());
        }
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let config: Configuration = json(response, 8 * 1024).await?;
        if self.site_destination(scope)? != (relay.clone(), audience.clone()) {
            return Err(stale());
        }
        configuration(&config, &relay, &audience)?;
        Ok(Some((relay, config)))
    }

    async fn site_inspect_at(
        &self,
        scope: &SiteScope,
        relay: &str,
        config: &Configuration,
        source: &SourceVersion,
    ) -> R<Inspection> {
        let signed = {
            let e = self.lock();
            let selected = e.site_source(scope, &source.item_id, source.version, &source.sha256)?;
            e.site_scope(scope, true)?;
            if selected.source != *source {
                return Err(stale());
            }
            let mut signed = SignedInspection {
                owner_id: scope.identity_id.clone(),
                device_id: scope.device_id.clone(),
                at_ms: safe_time()?,
                site_id: protocol::sites::site_id(
                    &scope.identity_id,
                    &scope.space_id,
                    &source.item_id,
                    &config.public_origin,
                )
                .map_err(|_| invalid())?,
                public_origin: config.public_origin.clone(),
                before: None,
                signature: String::new(),
            };
            let signing_bytes = signed
                .signing_bytes(&config.audience)
                .map_err(|_| invalid())?;
            signed.signature = e
                .net
                .author
                .as_ref()
                .ok_or_else(forbidden)?
                .key
                .sign(&signing_bytes);
            signed
        };
        let response = http()?
            .post(format!("{}/v1/sites/inspect", config.management_origin))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(&signed).map_err(|_| invalid())?)
            .send()
            .await
            .map_err(|_| unavailable())?;
        let inspection: Inspection = json(response, 256 * 1024).await?;
        if self.site_destination(scope)? != (relay.into(), config.audience.clone()) {
            return Err(stale());
        }
        self.lock().site_scope(scope, true)?;
        validate_inspection(&inspection, &signed)?;
        Ok(inspection)
    }

    pub(crate) async fn site_status(
        &self,
        scope: SiteScope,
        source: SourceVersion,
    ) -> R<Option<SiteStatus>> {
        let Some((relay, config)) = self.site_configuration(&scope).await? else {
            return Ok(None);
        };
        let inspection = self
            .site_inspect_at(&scope, &relay, &config, &source)
            .await?;
        let e = self.lock();
        let mut pending = e.site_pending(&scope, &inspection.site_id)?;
        if let Some(review) = &pending {
            let receipt = inspection
                .receipts
                .iter()
                .find(|r| r.request_id == review.mutation.request_id);
            if let Some(receipt) = receipt {
                e.site_publication_result(review, Some(receipt))?;
                pending = None;
            }
        }
        Ok(Some(SiteStatus {
            configuration: config,
            inspection,
            pending,
        }))
    }

    pub(crate) async fn site_review(
        &self,
        scope: SiteScope,
        source: SourceVersion,
        unpublish: bool,
    ) -> R<SiteReview> {
        let (relay, config) = self
            .site_configuration(&scope)
            .await?
            .ok_or_else(unavailable)?;
        let inspection = self
            .site_inspect_at(&scope, &relay, &config, &source)
            .await?;
        let e = self.lock();
        e.site_scope(&scope, true)?;
        if let Some(pending) = e.site_pending(&scope, &inspection.site_id)? {
            return Ok(pending);
        }
        let selected = e.site_source(&scope, &source.item_id, source.version, &source.sha256)?;
        if selected.source != source
            || (!unpublish && selected.html.is_none())
            || (unpublish && !inspection.active)
        {
            return Err(stale());
        }
        Ok(SiteReview {
            scope,
            source: source.clone(),
            relay_url: relay,
            configuration: config.clone(),
            mutation: Mutation {
                schema: SCHEMA,
                public_origin: config.public_origin,
                site_id: inspection.site_id,
                request_id: types::new_secret_id("pub")?,
                expected_head: inspection.head,
                change: if unpublish {
                    Change::Unpublish
                } else {
                    Change::Publish { source }
                },
            },
        })
    }

    pub(crate) async fn site_prepare(&self, review: SiteReview) -> R<SitePrepared> {
        let (relay, config) = self
            .site_configuration(&review.scope)
            .await?
            .ok_or_else(unavailable)?;
        let result = {
            let e = self.lock();
            let result = e.site_prepare(&review, &relay, &config)?;
            // The exact intent is encrypted durably before its signed request
            // leaves the Core. No new request ID is invented on an ambiguous error.
            e.site_begin_publication(&review)?;
            result
        };
        if self.site_destination(&review.scope)? != (relay, config.audience) {
            return Err(stale());
        }
        Ok(result)
    }

    pub(crate) async fn site_publish(&self, review: SiteReview) -> R<Receipt> {
        let prepared = self.site_prepare(review.clone()).await?;
        #[derive(Serialize)]
        struct Body<'a> {
            signed: &'a SignedMutation,
            html: &'a Option<String>,
        }
        let response = http()?
            .post(format!(
                "{}/v1/sites/publications",
                prepared.configuration.management_origin
            ))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(
                serde_json::to_vec(&Body {
                    signed: &prepared.signed,
                    html: &prepared.html,
                })
                .map_err(|_| invalid())?,
            )
            .send()
            .await
            .map_err(|_| unavailable())?;
        if self.site_destination(&review.scope)?
            != (
                prepared.relay_url.clone(),
                prepared.configuration.audience.clone(),
            )
        {
            return Err(stale());
        }
        if response.status() == reqwest::StatusCode::CONFLICT {
            // This authenticated response proves this exact request was refused.
            // Keep all ambiguous/network failures pending instead.
            self.lock().site_publication_result(&review, None)?;
            return Err(stale());
        }
        let receipt: Receipt = json(response, 16 * 1024).await?;
        if self.site_destination(&review.scope)?
            != (prepared.relay_url, prepared.configuration.audience.clone())
        {
            return Err(stale());
        }
        let e = self.lock();
        e.site_scope(&review.scope, true)?;
        e.site_publication_result(&review, Some(&receipt))?;
        Ok(receipt)
    }
}

fn validate_inspection(value: &Inspection, signed: &SignedInspection) -> R<()> {
    if value.site_id != signed.site_id
        || value.receipts.len() > 100
        || value.head.as_deref().is_some_and(|h| !hex64(h))
        || value.next_before.as_deref().is_some_and(|h| !hex64(h))
    {
        return Err(unavailable());
    }
    let mut previous = value.head.clone();
    for receipt in &value.receipts {
        if receipt.schema != SCHEMA
            || receipt.owner_id != signed.owner_id
            || receipt.site_id != signed.site_id
            || previous.as_ref() != Some(&receipt.head)
            || !hex64(&receipt.head)
            || receipt.active != receipt.source.is_some()
            || receipt
                .source
                .as_ref()
                .is_some_and(|source| !valid_source(source))
        {
            return Err(unavailable());
        }
        previous = receipt.previous_head.clone();
    }
    if value.head.is_none()
        && (value.active || !value.receipts.is_empty() || value.next_before.is_some())
        || value.head.is_some()
            && value
                .receipts
                .first()
                .is_none_or(|r| r.active != value.active)
        || previous != value.next_before
    {
        return Err(unavailable());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovered_destinations_bind_audience_https_and_isolated_canonical_public_host() {
        let config = Configuration {
            schema: SCHEMA,
            audience: "relay.test".into(),
            management_origin: "https://api.test".into(),
            public_origin: "https://sites.test".into(),
            max_html_bytes: MAX_HTML_BYTES,
        };
        assert!(configuration(&config, "https://relay.test", "relay.test").is_ok());
        for url in [
            "http://remote.test",
            "https://user:pass@sites.test",
            "https://sites.test/path",
            "https://sites.test?q=x",
            "https://sites.test#x",
            "https://sites.test/",
        ] {
            assert!(
                configuration(
                    &Configuration {
                        public_origin: url.into(),
                        ..config.clone()
                    },
                    "https://relay.test",
                    "relay.test"
                )
                .is_err(),
                "{url}"
            );
        }
        assert!(configuration(&config, "https://relay.test", "other").is_err());
        assert!(configuration(
            &Configuration {
                public_origin: "https://api.test".into(),
                ..config.clone()
            },
            "https://relay.test",
            "relay.test"
        )
        .is_err());
        assert!(configuration(
            &Configuration {
                public_origin: "https://relay.test".into(),
                ..config
            },
            "https://relay.test",
            "relay.test"
        )
        .is_err());
    }
}
