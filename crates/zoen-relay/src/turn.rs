//! RFC TURN REST credentials: short-lived, opaque per-device allocation identities.
//! An allocation is transport only; consent/DTLS fingerprints remain inside MLS.
use crate::limits::{Limiter, Quota};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use protocol::turn::{valid_urls, TurnCredentials, CREDENTIAL_TTL_SECONDS};
use ring::hmac;

pub struct Service {
    urls: Vec<String>,
    key: Option<hmac::Key>,
    identity_key: Option<hmac::Key>,
    device: Limiter,
    account: Limiter,
}

impl Service {
    pub fn disabled() -> Self {
        Self {
            urls: Vec::new(),
            key: None,
            identity_key: None,
            device: Limiter::new("call_turn_device", Quota::per_minute(6, 4)),
            account: Limiter::new("call_turn_account", Quota::per_minute(12, 8)),
        }
    }
    pub fn from_env() -> anyhow::Result<Self> {
        Self::configured(
            std::env::var("ZOEN_TURN_URLS").ok(),
            std::env::var("ZOEN_TURN_REST_SECRET").ok(),
        )
    }
    fn configured(urls: Option<String>, secret: Option<String>) -> anyhow::Result<Self> {
        let mut service = Self::disabled();
        let (urls, secret) = match (urls, secret) {
            (None, None) => return Ok(service),
            (Some(urls), Some(secret)) => (urls, secret),
            _ => anyhow::bail!("TURN URLs and shared secret must be configured together"),
        };
        let urls: Vec<String> = urls.split(',').map(str::trim).map(str::to_owned).collect();
        if !valid_urls(&urls)
            || secret.len() < 32
            || secret.len() > 256
            || secret.chars().any(char::is_whitespace)
        {
            anyhow::bail!("TURN configuration is invalid");
        }
        service.urls = urls;
        // SHA-1 here is the existing coturn REST credential MAC, not a document
        // digest or a new cryptographic protocol. TURN expects base64(HMAC-SHA1).
        service.key = Some(hmac::Key::new(
            hmac::HMAC_SHA1_FOR_LEGACY_USE_ONLY,
            secret.as_bytes(),
        ));
        service.identity_key = Some(hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes()));
        Ok(service)
    }
    pub(crate) fn check_quota(&self, identity: &str, device: &str) -> Result<(), String> {
        if self.key.is_none() {
            return Err("call_transport_unavailable".into());
        }
        self.device
            .check(device)
            .map_err(crate::limits::slow_down)?;
        self.account
            .check(identity)
            .map_err(crate::limits::slow_down)
    }
    pub(crate) fn issue(
        &self,
        identity: &str,
        device: &str,
        now_ms: i64,
    ) -> Result<TurnCredentials, String> {
        let key = self.key.as_ref().ok_or("call_transport_unavailable")?;
        let now = now_ms
            .checked_div(1000)
            .filter(|n| *n > 0)
            .ok_or("call_transport_unavailable")?;
        // Stable within one minute, so requesting again cannot mint unbounded
        // distinct usernames to bypass coturn's per-user allocation quota.
        let expiry = (now / 60)
            .checked_mul(60)
            .and_then(|n| n.checked_add(CREDENTIAL_TTL_SECONDS))
            .ok_or("call_transport_unavailable")?;
        let identity_key = self
            .identity_key
            .as_ref()
            .ok_or("call_transport_unavailable")?;
        let opaque = hex::encode(
            hmac::sign(
                identity_key,
                &serde_json::to_vec(&("zoen.turn.device.v1", identity, device))
                    .map_err(|_| "call_transport_unavailable")?,
            )
            .as_ref(),
        );
        let username = format!("{expiry}:{opaque}");
        let credential = STANDARD.encode(hmac::sign(key, username.as_bytes()).as_ref());
        Ok(TurnCredentials {
            urls: self.urls.clone(),
            username,
            credential,
            expires_at_ms: expiry
                .checked_mul(1000)
                .ok_or("call_transport_unavailable")?,
        })
    }
}

#[cfg(test)]
mod tests;
