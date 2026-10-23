use axum::{http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use protocol::sites::{Configuration, MAX_HTML_BYTES, SCHEMA};

use super::Error;

fn origin(value: &str) -> Result<reqwest::Url, Error> {
    let url = reqwest::Url::parse(value).map_err(|_| Error::Invalid)?;
    let local = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if !(url.scheme() == "https" || url.scheme() == "http" && local)
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || value.len() > 2048
    {
        return Err(Error::Invalid);
    }
    Ok(url)
}

pub fn configure(management: &str, public: &str, audience: &str) -> Result<Configuration, Error> {
    let management = origin(management)?;
    let public = origin(public)?;
    if management.host_str() == public.host_str() || audience.is_empty() || audience.len() > 2048 {
        return Err(Error::Invalid);
    }
    Ok(Configuration {
        schema: SCHEMA,
        audience: audience.into(),
        management_origin: management.origin().ascii_serialization(),
        public_origin: public.origin().ascii_serialization(),
        max_html_bytes: MAX_HTML_BYTES,
    })
}

pub(super) fn validate(config: &Configuration) -> Result<(), Error> {
    if config
        != &configure(
            &config.management_origin,
            &config.public_origin,
            &config.audience,
        )?
    {
        return Err(Error::Invalid);
    }
    Ok(())
}

/// Discover only on the client's already known TLS relay. Configuration is not
/// supplied by an HTML document or an agent, and never authorizes publication.
pub(crate) fn announcement(audience: &str) -> Router {
    let settings = match (
        std::env::var("ZOEN_SITES_MANAGEMENT_ORIGIN"),
        std::env::var("ZOEN_SITES_PUBLIC_ORIGIN"),
    ) {
        (Err(std::env::VarError::NotPresent), Err(std::env::VarError::NotPresent)) => Ok(None),
        (Ok(management), Ok(public)) => configure(&management, &public, audience).map(Some),
        _ => Err(Error::Invalid),
    };
    Router::new().route(
        "/v1/sites/config",
        get(move || {
            let settings = settings.clone();
            async move {
                match settings {
                    Ok(Some(config)) => Json(config).into_response(),
                    Ok(None) => StatusCode::NOT_FOUND.into_response(),
                    Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
                }
            }
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sites_origins_are_canonical_separate_and_https_except_loopback() {
        let settings = configure(
            "https://API.example.test/",
            "https://sites.example.test/",
            "relay",
        )
        .unwrap();
        assert_eq!(settings.management_origin, "https://api.example.test");
        assert!(validate(&settings).is_ok());
        assert!(configure("http://localhost:8787", "http://127.0.0.1:8788", "relay").is_ok());
        for bad in [
            "https://api.example.test:99",
            "http://elsewhere.test",
            "https://user:pass@sites.example.test",
            "https://sites.example.test/path",
            "https://sites.example.test/?q=1",
        ] {
            assert_eq!(
                configure("https://api.example.test", bad, "relay"),
                Err(Error::Invalid)
            );
        }
    }
}
