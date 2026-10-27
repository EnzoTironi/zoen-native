//! HTTPS clients for the Core.
//!
//! Trust is the Mozilla root set compiled into `webpki-roots`, verified by
//! rustls with the ring provider. The WebSocket path trusts that same set
//! (`rustls-tls-webpki-roots`; its 0.26 crate re-exports these roots). The
//! config carries its own provider, so a request does not call
//! `rustls-platform-verifier` and does not need an Android `Context`.

use std::sync::{Arc, OnceLock};

/// A reqwest builder that verifies server certificates with the bundled roots.
/// Returns a storage/configuration error when the compiled provider cannot
/// support the required protocols; never falls back to a weaker trust policy.
pub(crate) fn client() -> Result<reqwest::ClientBuilder, crate::CoreError> {
    Ok(reqwest::Client::builder().tls_backend_preconfigured(tls_config()?.clone()))
}

fn tls_config() -> Result<&'static rustls::ClientConfig, crate::CoreError> {
    static CONFIG: OnceLock<Result<rustls::ClientConfig, String>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            build_config(Arc::new(rustls::crypto::ring::default_provider()))
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|message| crate::CoreError::Storage {
            message: format!("TLS configuration unavailable: {message}"),
        })
}

fn build_config(
    provider: Arc<rustls::crypto::CryptoProvider>,
) -> Result<rustls::ClientConfig, rustls::Error> {
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_root_certificates(mozilla_roots())
        .with_no_client_auth();
    // reqwest is built without HTTP/2, so advertise only HTTP/1.1.
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(config)
}

fn mozilla_roots() -> rustls::RootCertStore {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    roots
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::net::{Ipv4Addr, SocketAddr, TcpListener};
    use std::sync::Arc;
    use std::time::Duration;

    use super::client;
    use crate::CoreError;

    #[test]
    fn provider_without_supported_cipher_suites_is_refused() {
        let mut provider = rustls::crypto::ring::default_provider();
        provider.cipher_suites.clear();
        assert!(super::build_config(Arc::new(provider)).is_err());
    }

    #[test]
    fn provider_without_key_exchange_groups_is_refused() {
        let mut provider = rustls::crypto::ring::default_provider();
        provider.kx_groups.clear();
        assert!(super::build_config(Arc::new(provider)).is_err());
    }

    #[test]
    fn bundled_roots_include_the_mozilla_isrg_root() {
        let roots = super::mozilla_roots();
        assert!(
            roots.roots.len() > 100,
            "expected the Mozilla bundle, got {} roots",
            roots.roots.len()
        );
        let isrg = roots.roots.iter().any(|anchor| {
            anchor
                .subject
                .as_ref()
                .windows(b"ISRG Root X1".len())
                .any(|window| window == b"ISRG Root X1")
        });
        assert!(isrg, "Mozilla bundle is missing ISRG Root X1");
    }

    #[tokio::test]
    async fn shared_client_refuses_a_self_signed_server_with_a_core_error(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let addr = untrusted_loopback();
        let http = client()?
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|err| CoreError::Invalid {
                reason: err.to_string(),
            })
            .expect("shared client builds without a platform verifier");
        let result = http.get(format!("https://{addr}/")).send().await;
        let err = match result {
            Ok(response) => panic!("accepted an untrusted certificate: {}", response.status()),
            Err(err) => err,
        };
        let refused = CoreError::Invalid {
            reason: error_chain(&err),
        };
        match refused {
            CoreError::Invalid { reason } => {
                let lower = reason.to_ascii_lowercase();
                assert!(
                    lower.contains("certificate") && lower.contains("unknownissuer"),
                    "{reason}"
                );
                assert!(!lower.contains("platform-verifier"), "{reason}");
            }
            other => panic!("untrusted certificate must be CoreError::Invalid, got {other}"),
        }

        Ok(())
    }

    fn error_chain(err: &reqwest::Error) -> String {
        let mut parts = vec![err.to_string()];
        let mut source = std::error::Error::source(err);
        while let Some(next) = source {
            parts.push(next.to_string());
            source = std::error::Error::source(next);
        }
        parts.join("\n")
    }

    fn untrusted_loopback() -> SocketAddr {
        let key = rcgen::KeyPair::generate().expect("test key");
        let mut params =
            rcgen::CertificateParams::new(vec!["127.0.0.1".to_string()]).expect("test name");
        params.key_usages = vec![rcgen::KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
        let cert = params.self_signed(&key).expect("test cert");
        let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(
            rustls::pki_types::PrivatePkcs8KeyDer::from(key.serialize_der()),
        );
        let server = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("tls versions")
        .with_no_client_auth()
        .with_single_cert(vec![cert.der().clone()], key_der)
        .expect("server identity");
        let server = Arc::new(server);
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
            sock.set_read_timeout(Some(Duration::from_secs(5))).ok();
            sock.set_write_timeout(Some(Duration::from_secs(5))).ok();
            let mut conn = rustls::ServerConnection::new(server).expect("server conn");
            let mut tls = rustls::Stream::new(&mut conn, &mut sock);
            let mut buf = [0u8; 8];
            let _ = tls.read(&mut buf);
        });
        addr
    }
}
