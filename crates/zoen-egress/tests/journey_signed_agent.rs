//! Journey (ADR 0028 §7): Ana's browser agent visits a shop that only lets verified agents
//! through (Cloudflare's "signed agents"). The egress proxy signs each request with Zoen's
//! Web Bot Auth key; the shop fetches Zoen's key directory, checks the signature and lets the
//! agent in instead of showing a CAPTCHA. The sandbox never holds the key and can't forge a
//! signature of its own; a tool that doesn't sign sends nothing.

use rcgen::{BasicConstraints, CertificateParams, IsCa, Issuer, KeyPair};
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use zoen_egress::tls::pem_certs;
use zoen_egress::webbotauth::{verify_directory, verify_request, DIRECTORY_PATH};
use zoen_egress::{
    ApprovalSink, Egress, EgressConfig, EgressRule, LeasePolicy, Resolver, SecretSource,
    SignedAgent,
};

/// Every request's headers, as the shop received them.
type Seen = Arc<Mutex<Vec<Vec<(String, String)>>>>;

type Failure = Box<dyn std::error::Error + Send + Sync>;
type Check<T> = Result<T, Failure>;

struct NoSecrets;
impl SecretSource for NoSecrets {
    fn secret(&self, _: &str, _: &str) -> Option<String> {
        None
    }
}
struct NoCards;
impl ApprovalSink for NoCards {
    fn opened(&self, _: &str, _: &types::AgentRequest) {}
}

fn now_s() -> Check<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

async fn read_request<S: AsyncRead + Unpin>(s: &mut S) -> Check<(String, Vec<(String, String)>)> {
    let mut buf = vec![];
    let mut chunk = [0u8; 4096];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = s.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let text = String::from_utf8_lossy(&buf).to_string();
    let mut lines = text.split("\r\n");
    let first = lines.next().unwrap_or_default().to_string();
    let headers = lines
        .take_while(|l| !l.is_empty())
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    Ok((first, headers))
}

fn header<'a>(h: &'a [(String, String)], name: &str) -> Option<&'a str> {
    h.iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Zoen's key directory, served from `agents.zoen.test` (plain HTTP on loopback here).
async fn directory(agent: Arc<SignedAgent>) -> Check<u16> {
    let l = TcpListener::bind("127.0.0.1:0").await?;
    let port = l.local_addr()?.port();
    tokio::spawn(async move {
        while let Ok((mut c, _)) = l.accept().await {
            let agent = agent.clone();
            tokio::spawn(async move {
                let (first, h) = read_request(&mut c).await?;
                assert!(
                    first.starts_with(&format!("GET {DIRECTORY_PATH} ")),
                    "{first}"
                );
                let host = header(&h, "host").unwrap_or_default().to_string();
                let (headers, body) = agent.directory_response(&host, now_s()?)?;
                let mut resp = String::from("HTTP/1.1 200 OK\r\n");
                for (n, v) in headers {
                    resp.push_str(&n);
                    resp.push_str(": ");
                    resp.push_str(&v);
                    resp.push_str("\r\n");
                }
                resp.push_str("Content-Length: ");
                resp.push_str(&body.len().to_string());
                resp.push_str("\r\nConnection: close\r\n\r\n");
                resp.push_str(&body);
                c.write_all(resp.as_bytes()).await?;
                Ok::<(), Failure>(c.shutdown().await?)
            });
        }
    });
    Ok(port)
}

/// What the shop does with a request: fetch the directory named by `Signature-Agent`, check
/// it, then check the request's signature against it.
async fn check(headers: &[(String, String)], dir_port: u16) -> Check<String> {
    let agent = header(headers, "signature-agent").ok_or("unsigned")?;
    let url = agent.trim_matches('"');
    let dir_host = url.strip_prefix("https://").ok_or("agent must be https")?;
    let mut c = TcpStream::connect(("127.0.0.1", dir_port)).await?;
    c.write_all(
        format!("GET {DIRECTORY_PATH} HTTP/1.1\r\nHost: {dir_host}\r\nConnection: close\r\n\r\n")
            .as_bytes(),
    )
    .await?;
    let mut raw = vec![];
    c.read_to_end(&mut raw).await?;
    let raw = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .ok_or("Missing directory response headers")?;
    let dh: Vec<(String, String)> = head
        .split("\r\n")
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    let jwks = verify_directory(&dh, body, dir_host, now_s()?)?;
    let authority = header(headers, "host").ok_or("no host")?;
    verify_request(headers, authority, &jwks, now_s()?)
        .map(|v| v.keyid)
        .map_err(Into::into)
}

/// The shop: answers "verified <keyid>" or "challenge: <why>" (where it would show a
/// CAPTCHA), over TLS when `tls` is set.
async fn shop(tls: Option<tokio_rustls::TlsAcceptor>, dir_port: u16, seen: Seen) -> Check<u16> {
    let l = TcpListener::bind("127.0.0.1:0").await?;
    let port = l.local_addr()?.port();
    tokio::spawn(async move {
        while let Ok((c, _)) = l.accept().await {
            let tls = tls.clone();
            let seen = seen.clone();
            tokio::spawn(async move {
                async fn serve<S: AsyncRead + AsyncWrite + Unpin>(
                    mut s: S,
                    dir_port: u16,
                    seen: Seen,
                ) -> Check<()> {
                    let (_, h) = read_request(&mut s).await?;
                    let verdict = match check(&h, dir_port).await {
                        Ok(k) => format!("verified {k}"),
                        Err(e) => format!("challenge: {e}"),
                    };
                    seen.lock().map_err(|error| error.to_string())?.push(h);
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{verdict}",
                        verdict.len()
                    );
                    s.write_all(resp.as_bytes()).await?;
                    Ok(s.shutdown().await?)
                }
                match tls {
                    Some(a) => serve(a.accept(c).await?, dir_port, seen).await,
                    None => serve(c, dir_port, seen).await,
                }
            });
        }
    });
    Ok(port)
}

/// The shop's own certificate, from a test CA the proxy trusts upstream.
fn shop_tls() -> Check<(tokio_rustls::TlsAcceptor, String)> {
    let ca_key = KeyPair::generate()?;
    let mut ca = CertificateParams::new(Vec::<String>::new())?;
    ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    let ca_cert = ca.self_signed(&ca_key)?;
    let issuer = Issuer::new(ca, ca_key);
    let key = KeyPair::generate()?;
    let leaf =
        CertificateParams::new(vec!["shop.example.test".into()])?.signed_by(&key, &issuer)?;
    let cfg = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(
        vec![leaf.der().clone()],
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
    )?;
    Ok((
        tokio_rustls::TlsAcceptor::from(Arc::new(cfg)),
        ca_cert.pem(),
    ))
}

/// The browser in the sandbox: CONNECT, then TLS trusting only the node's sandbox root.
async fn browser_get(
    proxy: u16,
    lease: &str,
    host: &str,
    port: u16,
    root_pem: &str,
    extra: &str,
) -> Check<String> {
    let mut c = TcpStream::connect(("127.0.0.1", proxy)).await?;
    let auth = base64_auth(lease);
    c.write_all(
        format!("CONNECT {host}:{port} HTTP/1.1\r\nProxy-Authorization: Basic {auth}\r\n\r\n")
            .as_bytes(),
    )
    .await?;
    let mut head = [0u8; 39];
    c.read_exact(&mut head).await?;
    assert!(String::from_utf8_lossy(&head).starts_with("HTTP/1.1 200"));
    let mut roots = rustls::RootCertStore::empty();
    for d in pem_certs(root_pem)? {
        roots.add(d)?;
    }
    let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let mut s = tokio_rustls::TlsConnector::from(Arc::new(cfg))
        .connect(ServerName::try_from(host.to_string())?, c)
        .await
        .map_err(|error| {
            format!(
                "The browser must trust the lease certificate through the sandbox root: {error}"
            )
        })?;
    s.write_all(format!("GET /checkout HTTP/1.1\r\nHost: {host}:{port}\r\n{extra}\r\n").as_bytes())
        .await?;
    let mut out = vec![];
    let _ = s.read_to_end(&mut out).await;
    Ok(String::from_utf8_lossy(&out).to_string())
}

fn base64_auth(lease: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(format!("{lease}:t"))
}

#[tokio::test]
async fn a_signed_agent_gets_through_and_nobody_else_can_sign() {
    let agent = Arc::new(SignedAgent::generate("https://agents.zoen.test").unwrap());
    let dir_port = directory(agent.clone()).await.unwrap();
    let (acceptor, shop_ca) = shop_tls().unwrap();
    let seen = Arc::new(Mutex::new(vec![]));
    let secure_port = shop(Some(acceptor), dir_port, seen.clone()).await.unwrap();
    let plain_port = shop(None, dir_port, seen.clone()).await.unwrap();

    let lo = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let egress = Egress::new(
        EgressConfig {
            allow_loopback_upstreams: true,
            extra_upstream_roots_pem: vec![shop_ca],
            signed_agent: Some(agent.clone()),
            ..Default::default()
        },
        Resolver::Static(HashMap::from([("shop.example.test".to_string(), vec![lo])])),
        Arc::new(NoSecrets),
        Arc::new(NoCards),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pport = listener.local_addr().unwrap().port();
    tokio::spawn(egress.clone().serve(listener));
    let mut rule = EgressRule::host("shop.example.test");
    rule.ports = vec![secure_port, plain_port];
    egress
        .register(policy(&rule, "lease_browser", true))
        .unwrap();
    egress
        .register(policy(&rule, "lease_plain", false))
        .unwrap();
    let root = egress.sandbox_root_pem();

    // 1. Over HTTPS: the browser trusts only the node root; the proxy intercepts, signs, and
    //    the shop verifies against Zoen's directory. A signature the page tried to add itself
    //    is dropped first.
    let r = browser_get(
        pport,
        "lease_browser",
        "shop.example.test",
        secure_port,
        &root,
        "Signature: sig1=:AAAA:\r\nSignature-Agent: \"https://evil.test\"\r\n",
    )
    .await
    .unwrap();
    assert!(r.ends_with(&format!("verified {}", agent.keyid())), "{r}");
    {
        let got = seen.lock().unwrap();
        let h = got.last().unwrap();
        assert_eq!(
            h.iter()
                .filter(|(n, _)| n.eq_ignore_ascii_case("signature"))
                .count(),
            1
        );
        assert_eq!(
            header(h, "signature-agent"),
            Some("\"https://agents.zoen.test\"")
        );
        assert!(header(h, "signature-input")
            .unwrap()
            .contains("tag=\"web-bot-auth\""));
    }

    // 2. Plain HTTP is signed too.
    let mut c = TcpStream::connect(("127.0.0.1", pport)).await.unwrap();
    c.write_all(
        format!(
            "GET http://shop.example.test:{plain_port}/ HTTP/1.1\r\nHost: shop.example.test:{plain_port}\r\nProxy-Authorization: Basic {}\r\n\r\n",
            base64_auth("lease_browser")
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    let mut out = vec![];
    c.read_to_end(&mut out).await.unwrap();
    let out = String::from_utf8_lossy(&out);
    assert!(
        out.ends_with(&format!("verified {}", agent.keyid())),
        "{out}"
    );

    // 3. A lease that doesn't sign sends nothing, and the shop challenges it.
    let mut c = TcpStream::connect(("127.0.0.1", pport)).await.unwrap();
    c.write_all(
        format!(
            "GET http://shop.example.test:{plain_port}/ HTTP/1.1\r\nHost: shop.example.test:{plain_port}\r\nProxy-Authorization: Basic {}\r\nSignature-Agent: \"https://agents.zoen.test\"\r\n\r\n",
            base64_auth("lease_plain")
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    let mut out = vec![];
    c.read_to_end(&mut out).await.unwrap();
    let out = String::from_utf8_lossy(&out);
    assert!(out.contains("challenge:"), "{out}");

    // 4. The log stays metadata only: no signature, no path.
    let text = serde_json::to_string(&egress.log().unwrap()).unwrap();
    assert!(!text.contains("sig1") && !text.contains("/checkout"));
    println!(
        "signed agent: keyid {} verified over HTTPS and HTTP",
        agent.keyid()
    );
}

fn policy(rule: &EgressRule, lease: &str, sign: bool) -> LeasePolicy {
    LeasePolicy {
        lease: lease.into(),
        token: "t".into(),
        owner: "ana".into(),
        agent: "ana-agent".into(),
        tool: "browser".into(),
        rules: vec![rule.clone()],
        secrets: vec![],
        sign_requests: sign,
        browser: true,
    }
}
