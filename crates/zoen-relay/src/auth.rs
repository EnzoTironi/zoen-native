//! Provider authentication proves a provider subject. The signed bind separately
//! proves control of Zoen keys; email never selects, merges or recovers an identity.

mod jwt;
mod storage;

use crate::{ClientIp, Shared};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use relay_admission::auth::enrollment::{self, Audience, Challenge};
use serde::Serialize;
use std::collections::BTreeMap;

pub use relay_admission::auth::enrollment::{ChallengeRequest, Client, VerifyRequest};
pub use relay_admission::auth::Provider;
#[derive(Clone, Default)]
pub struct Settings {
    audiences: BTreeMap<(Provider, Client), Audience>,
    origins: Vec<HeaderValue>,
}
impl Settings {
    pub fn from_env() -> anyhow::Result<Self> {
        let mut settings = Self::default();
        for (provider, prefix) in [(Provider::Apple, "APPLE"), (Provider::Google, "GOOGLE")] {
            for (client, suffix) in [
                (Client::Ios, "IOS"),
                (Client::Android, "ANDROID"),
                (Client::Web, "WEB"),
            ] {
                // Apple's native SDK is iOS-only. Android Google uses its own
                // registered app/presenter; it never inherits iOS configuration.
                if provider == Provider::Apple && client == Client::Android {
                    continue;
                }
                let key = format!("ZOEN_AUTH_{prefix}_{suffix}_AUDIENCE");
                if let Ok(value) = std::env::var(&key) {
                    let value = value.trim();
                    if !enrollment::valid_audience_value(value) {
                        anyhow::bail!("Invalid {key}");
                    }
                    let presenter = std::env::var(format!("ZOEN_AUTH_{prefix}_{suffix}_PRESENTER"))
                        .ok()
                        .map(|value| value.trim().to_string());
                    if presenter
                        .as_ref()
                        .is_some_and(|value| !enrollment::valid_audience_value(value))
                        || client == Client::Android && presenter.is_none()
                    {
                        anyhow::bail!("Invalid provider presenter");
                    }
                    settings.audiences.insert(
                        (provider, client),
                        Audience {
                            id: value.into(),
                            presenter,
                        },
                    );
                }
            }
        }
        for origin in std::env::var("ZOEN_AUTH_WEB_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let url = reqwest::Url::parse(origin)?;
            if !["https", "http"].contains(&url.scheme())
                || url.host_str().is_none()
                || url.origin().ascii_serialization() != origin
                || url.scheme() == "http"
                    && !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
            {
                anyhow::bail!(
                    "Auth web origins must be exact HTTPS origins (HTTP loopback is allowed)"
                );
            }
            settings.origins.push(HeaderValue::from_str(origin)?);
        }
        Ok(settings)
    }
}
pub struct Service {
    settings: Settings,
    verifier: jwt::Verifier,
}
impl Service {
    pub fn new(settings: Settings) -> anyhow::Result<Self> {
        Ok(Self {
            settings,
            verifier: jwt::Verifier::new()?,
        })
    }
    pub fn disabled() -> Self {
        Self::new(Settings::default()).expect("provider HTTP configuration")
    }
    fn origin(
        &self,
        headers: &HeaderMap,
        client: Option<Client>,
    ) -> Result<Option<String>, Failure> {
        let supplied = headers
            .get(header::ORIGIN)
            .map(|value| value.to_str())
            .transpose()
            .map_err(|_| error(StatusCode::FORBIDDEN, "origin_refused"))?;
        enrollment::origin(
            supplied,
            client,
            self.settings
                .origins
                .iter()
                .filter_map(|value| value.to_str().ok()),
        )
        .map_err(Into::into)
    }
}
pub fn router(service: &Service) -> Router<Shared> {
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(service.settings.origins.clone())
        .allow_methods([Method::POST])
        .allow_headers([header::CONTENT_TYPE]);
    Router::new()
        .route("/v1/auth/challenge", post(challenge))
        .route("/v1/auth/verify", post(verify))
        .route("/v1/auth/bind", post(bind))
        .layer(DefaultBodyLimit::max(32_768))
        .layer(cors)
        .layer(axum::middleware::from_fn(response_policy))
}

async fn response_policy(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let mut response = tokio::time::timeout(std::time::Duration::from_secs(10), next.run(request))
        .await
        .unwrap_or_else(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "authentication_unavailable",
            )
            .into_response()
        });
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}
#[derive(Serialize)]
struct ChallengeResponse {
    id: String,
    nonce: String,
    state: String,
    audience: String,
    expires_ms: i64,
}
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum Account {
    Unbound,
    Bound { identity_id: String, handle: String },
}
#[derive(Serialize)]
struct VerifyResponse {
    proof_token: String,
    expires_ms: i64,
    account: Account,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}
fn random() -> Result<String, Failure> {
    let mut bytes = [0; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|_| error(StatusCode::SERVICE_UNAVAILABLE, "random_unavailable"))?;
    Ok(hex::encode(bytes))
}
fn hash(value: &str) -> String {
    enrollment::token_hash(value)
}
#[derive(Debug)]
struct Failure {
    status: StatusCode,
    code: &'static str,
    retry_after: Option<u64>,
}
impl From<enrollment::Failure> for Failure {
    fn from(value: enrollment::Failure) -> Self {
        error(
            StatusCode::from_u16(value.status).expect("shared provider status is valid"),
            value.code,
        )
    }
}
impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let mut response =
            (self.status, Json(serde_json::json!({"error":self.code}))).into_response();
        if let Some(seconds) = self.retry_after {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        response
    }
}
fn error(status: StatusCode, code: &'static str) -> Failure {
    Failure {
        status,
        code,
        retry_after: None,
    }
}
fn unavailable(_: impl std::fmt::Display) -> Failure {
    error(
        StatusCode::SERVICE_UNAVAILABLE,
        "authentication_unavailable",
    )
}
fn limited(result: Result<(), std::time::Duration>) -> Result<(), Failure> {
    result.map_err(|wait| Failure {
        status: StatusCode::TOO_MANY_REQUESTS,
        code: "rate_limited",
        retry_after: Some(wait.as_secs().max(1)),
    })
}

async fn challenge(
    State(st): State<Shared>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(request): Json<ChallengeRequest>,
) -> Response {
    let run = async {
        limited(st.limits.auth_challenge_ip.check(&ip))?;
        let audience = enrollment::require_audience(
            st.auth
                .settings
                .audiences
                .get(&(request.provider, request.client)),
        )?;
        let origin = st.auth.origin(&headers, Some(request.client))?;
        let (id, nonce, state) = (random()?, random()?, random()?);
        let created = now();
        let expires = created + enrollment::CHALLENGE_LIFETIME_MS;
        // Proof rows cascade with old challenges. Cleanup is bounded by the short
        // expiry indexes and occurs only in this rate-limited lane.
        sqlx::query("DELETE FROM provider_challenges WHERE id IN (SELECT id FROM provider_challenges WHERE expires_ms < $1 LIMIT 500)")
            .bind(created - enrollment::CHALLENGE_CLEANUP_DELAY_MS)
            .execute(&st.pool)
            .await
            .map_err(unavailable)?;
        sqlx::query("INSERT INTO provider_challenges(id,provider,client,audience,presenter,origin,nonce,state,created_ms,expires_ms) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(&id).bind(request.provider.name()).bind(request.client.name()).bind(&audience.id).bind(&audience.presenter).bind(origin).bind(&nonce).bind(&state).bind(created).bind(expires).execute(&st.pool).await.map_err(unavailable)?;
        Ok::<_, Failure>(
            Json(ChallengeResponse {
                id,
                nonce,
                state,
                audience: audience.id.clone(),
                expires_ms: expires,
            })
            .into_response(),
        )
    };
    run.await.unwrap_or_else(IntoResponse::into_response)
}
async fn verify(
    State(st): State<Shared>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(request): Json<VerifyRequest>,
) -> Response {
    let run = async {
        limited(st.limits.auth_verify_ip.check(&ip))?;
        enrollment::validate_verify_request(&request)?;
        let challenge = storage::challenge(&st.pool, &request.challenge_id)
            .await
            .map_err(unavailable)?
            .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "challenge_expired"))?;
        let origin = st.auth.origin(&headers, Some(challenge.client))?;
        enrollment::validate_challenge(
            &challenge,
            &request.state,
            origin.as_deref(),
            st.auth
                .settings
                .audiences
                .get(&(challenge.provider, challenge.client)),
            now(),
        )?;
        let verified = st
            .auth
            .verifier
            .verify(&request.id_token, &challenge, now())
            .await
            .map_err(|_| error(StatusCode::UNAUTHORIZED, "invalid_provider_assertion"))?;
        let proof = random()?;
        let expires = now() + enrollment::PROOF_LIFETIME_MS;
        let account = storage::issue(
            &st.pool,
            &request.challenge_id,
            &challenge,
            &verified,
            &proof,
            expires,
            now(),
        )
        .await?;
        Ok::<_, Failure>(
            Json(VerifyResponse {
                proof_token: proof,
                expires_ms: expires,
                account,
                name: verified.name,
            })
            .into_response(),
        )
    };
    run.await.unwrap_or_else(IntoResponse::into_response)
}
async fn bind(
    State(st): State<Shared>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(request): Json<protocol::provider_auth::ProviderBindRequest>,
) -> Response {
    let run = async {
        limited(st.limits.auth_bind_ip.check(&ip))?;
        enrollment::require_provider_configured(!st.auth.settings.audiences.is_empty())?;
        let origin = st.auth.origin(&headers, None)?;
        let result = storage::bind(&st.pool, &request, origin.as_deref(), now()).await?;
        Ok::<_, Failure>(Json(result).into_response())
    };
    run.await.unwrap_or_else(IntoResponse::into_response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn auth_origins_are_explicit_and_web_challenges_cannot_omit_their_origin() {
        let service = Service::new(Settings {
            origins: vec![HeaderValue::from_static("https://app.example")],
            ..Default::default()
        })
        .unwrap();
        let mut headers = HeaderMap::new();
        assert!(service.origin(&headers, Some(Client::Web)).is_err());
        assert_eq!(service.origin(&headers, Some(Client::Ios)).unwrap(), None);
        assert_eq!(
            service.origin(&headers, Some(Client::Android)).unwrap(),
            None
        );
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://evil.example"),
        );
        assert!(service.origin(&headers, None).is_err());
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://app.example"),
        );
        assert_eq!(
            service
                .origin(&headers, Some(Client::Web))
                .unwrap()
                .as_deref(),
            Some("https://app.example")
        );
        assert!(Settings::default().audiences.is_empty());
    }
}
