//! Journey (ADR 0028 §6, P1): a tool in a microVM calls an API with the owner's key. The
//! VM's only way out is the egress proxy; the key goes in on the way out, over HTTPS, and
//! never exists inside the VM; anything off the list becomes a card for the owner.

mod common;

use common::*;
use roda_types::AgentRequest;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use zoen_agentd::{ExecRequest, SandboxProvider};
use zoen_egress::{
    placeholder, ApprovalScope, ApprovalSink, Egress, EgressConfig, EgressRule, Resolver,
    SecretBinding, SecretSource,
};
use zoen_sandboxd::FirecrackerProvider;

const KEY: &str = "sk_live_REAL_never_in_the_vm";

struct Vault;
impl SecretSource for Vault {
    fn secret(&self, owner: &str, name: &str) -> Option<String> {
        (owner == "id_ana" && name == "billing_api").then(|| KEY.to_string())
    }
}

#[derive(Default)]
struct Cards(Mutex<Vec<AgentRequest>>);
impl ApprovalSink for Cards {
    fn opened(&self, _lease: &str, r: &AgentRequest) {
        self.0.lock().unwrap().push(r.clone());
    }
}

/// Answers whether the request carried the real key, without ever echoing it, and records
/// every request head it saw.
fn answer(head: &str) -> Vec<u8> {
    let authorized = head
        .lines()
        .any(|l| l.eq_ignore_ascii_case(&format!("authorization: Bearer {KEY}")));
    let body = format!("{{\"authorized\":{authorized}}}");
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// An HTTPS origin for `api.billing.test` with its own self-signed certificate.
async fn https_origin() -> (u16, String, Arc<Mutex<Vec<String>>>) {
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = rcgen::CertificateParams::new(vec!["api.billing.test".to_string()])
        .unwrap()
        .self_signed(&key)
        .unwrap();
    let pem = cert.pem();
    let cfg = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(
        vec![CertificateDer::from(cert.der().to_vec())],
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())),
    )
    .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(cfg));
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(vec![]));
    let s = seen.clone();
    tokio::spawn(async move {
        while let Ok((c, _)) = l.accept().await {
            let acceptor = acceptor.clone();
            let s = s.clone();
            tokio::spawn(async move {
                let Ok(mut t) = acceptor.accept(c).await else {
                    return;
                };
                let mut buf = vec![0u8; 16384];
                let n = t.read(&mut buf).await.unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                s.lock().unwrap().push(head.clone());
                let _ = t.write_all(&answer(&head)).await;
                let _ = t.shutdown().await;
            });
        }
    });
    (port, pem, seen)
}

/// A plain-HTTP origin for the unlisted host.
async fn http_origin() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut c, _)) = l.accept().await {
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = c.read(&mut buf).await.unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = c.write_all(&answer(&head)).await;
                let _ = c.shutdown().await;
            });
        }
    });
    port
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_tool_in_a_microvm_uses_the_owners_key_over_https_without_ever_holding_it() {
    let Some(mut cfg) = config("journey-egress") else {
        return;
    };
    cfg.pool_size = 1;
    let (api_port, api_pem, api_seen) = https_origin().await;
    let docs_port = http_origin().await;
    let lo = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let cards = Arc::new(Cards::default());
    let egress = Egress::new(
        EgressConfig {
            allow_loopback_upstreams: true,
            extra_upstream_roots_pem: vec![api_pem],
            ..Default::default()
        },
        Resolver::Static(HashMap::from([
            ("api.billing.test".to_string(), vec![lo]),
            ("docs.billing.test".to_string(), vec![lo]),
        ])),
        Arc::new(Vault),
        cards.clone(),
    );
    let p = FirecrackerProvider::new(cfg, Some(egress.clone()));

    let mut spec = spec("tool.invoice-sync", 2, 256);
    let mut rule = EgressRule::host("api.billing.test");
    rule.ports = vec![api_port];
    spec.egress = vec![rule];
    spec.secrets = vec![SecretBinding {
        name: "billing_api".into(),
        hosts: vec!["api.billing.test".into()],
    }];
    spec.agent = Some("id_agent".into());
    let l = p.acquire(&spec, "id_ana").await.expect("acquire");
    let sh = |s: String| ExecRequest::sh(&s);
    let ph = placeholder("billing_api");
    let api = format!("https://api.billing.test:{api_port}/v1/invoices");

    // 1. curl over HTTPS (CONNECT through the in-VM proxy), with the placeholder: the origin
    //    gets the real key; the tool sees only the origin's answer. curl verifies the
    //    certificate, which the lease CA issued, against the VM's trust bundle.
    let t0 = Instant::now();
    let o = p
        .exec(
            &l,
            sh(format!(
                "curl -sS --fail -H 'Authorization: Bearer {ph}' {api}"
            )),
        )
        .await
        .unwrap();
    let first = t0.elapsed();
    assert_eq!(
        o.exit_code,
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(o.stdout_str(), "{\"authorized\":true}");
    assert!(api_seen.lock().unwrap().last().unwrap().contains(KEY));
    let mut lat = vec![];
    for _ in 0..5 {
        let t0 = Instant::now();
        let o = p
            .exec(
                &l,
                sh(format!(
                    "curl -sS --fail -H 'Authorization: Bearer {ph}' {api}"
                )),
            )
            .await
            .unwrap();
        assert_eq!(o.stdout_str(), "{\"authorized\":true}");
        lat.push(t0.elapsed());
    }
    lat.sort();
    eprintln!(
        "HTTPS call with key injection, from the VM: first {}, then p50 {}",
        ms(first),
        ms(lat[2])
    );

    // 2. busybox wget sends https:// in absolute form to the proxy, which opens the TLS
    //    connection itself; same result.
    let o = p
        .exec(
            &l,
            sh(format!(
                "wget -q -O - --header 'Authorization: Bearer {ph}' {api}"
            )),
        )
        .await
        .unwrap();
    assert_eq!(
        o.stdout_str(),
        "{\"authorized\":true}",
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );

    // 3. Nothing in the VM holds the key or the CA's private key: only the CA certificate.
    let o = p
        .exec(
            &l,
            sh(format!(
                "ls /run/zoen; grep -rl '{KEY}' / --exclude-dir=proc --exclude-dir=sys --exclude-dir=dev 2>/dev/null; \
                 grep -rl 'PRIVATE KEY' /run /work /tmp /root 2>/dev/null; env | grep -c '{KEY}'"
            )),
        )
        .await
        .unwrap();
    assert_eq!(o.stdout_str(), "ca-bundle.crt\nca.crt\n0\n");

    // 4. An unlisted host: refused, and the owner gets a card naming the agent; "once" lets
    //    exactly one request through.
    let docs = format!("http://docs.billing.test:{docs_port}/guide");
    let code = format!("curl -s -o /dev/null -w '%{{http_code}}' {docs}");
    assert_eq!(
        p.exec(&l, sh(code.clone())).await.unwrap().stdout_str(),
        "403"
    );
    let card = cards.lock_last();
    assert_eq!(card.agent, "id_agent");
    assert_eq!(card.title, "Permitir acesso a docs.billing.test");
    let _ = egress.approve(&card.id, ApprovalScope::Once);
    assert_eq!(
        p.exec(&l, sh(code.clone())).await.unwrap().stdout_str(),
        "200"
    );
    assert_eq!(p.exec(&l, sh(code)).await.unwrap().stdout_str(), "403");

    // 5. The key can't be sent anywhere else, even to an approved host.
    let card = cards.lock_last();
    let _ = egress.approve(&card.id, ApprovalScope::Task);
    let o = p
        .exec(
            &l,
            sh(format!("curl -s -H 'Authorization: Bearer {ph}' {docs}")),
        )
        .await
        .unwrap();
    assert!(
        o.stdout_str().contains("EGRESS_SECRET_NOT_BOUND"),
        "{}",
        o.stdout_str()
    );

    // 6. No way around the proxy: IP literals are refused by it, and without it there is no
    //    route at all (the VM has no network interface).
    let o = p
        .exec(
            &l,
            sh("curl -s http://169.254.169.254/latest/meta-data/".into()),
        )
        .await
        .unwrap();
    assert!(o.stdout_str().contains("EGRESS_IP_LITERAL"));
    let o = p
        .exec(
            &l,
            sh("curl -s -m 3 --noproxy '*' http://1.1.1.1/ >/dev/null 2>&1; echo rc=$?".into()),
        )
        .await
        .unwrap();
    assert_ne!(o.stdout_str().trim(), "rc=0");

    // 7. The log knows who went where, and nothing else.
    let log = serde_json::to_string(&egress.log()).unwrap();
    assert!(log.contains("api.billing.test") && log.contains(&l.id));
    assert!(!log.contains(KEY) && !log.contains("/v1/invoices") && !log.contains("zoen-secret"));

    // 8. End of task: the lease, its CA and its route are gone.
    p.release(l.clone()).await.unwrap();
    assert!(egress.lease_ca_pem(&l.id).is_none());
}

trait LastCard {
    fn lock_last(&self) -> AgentRequest;
}
impl LastCard for Arc<Cards> {
    fn lock_last(&self) -> AgentRequest {
        self.0.lock().unwrap().last().cloned().expect("a card")
    }
}
