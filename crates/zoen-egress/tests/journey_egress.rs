//! Journey (ADR 0028 §6): Ana's agent runs a tool that talks to the network. The tool reaches
//! only what its manifest declared; anything else becomes an approval card that Ana answers;
//! her GitHub token is used without ever entering the sandbox; the log holds metadata only.

use base64::Engine;
use roda_types::{AgentRequest, Capability};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use zoen_egress::{
    placeholder, ApprovalScope, ApprovalSink, Decision, Egress, EgressConfig, EgressRule,
    LeasePolicy, Resolver, SecretBinding, SecretSource,
};

const TOKEN: &str = "ghp_REAL_SECRET_never_in_sandbox";

struct Vault;
impl SecretSource for Vault {
    fn secret(&self, owner: &str, name: &str) -> Option<String> {
        (owner == "ana" && name == "github").then(|| TOKEN.to_string())
    }
}

#[derive(Default)]
struct Cards(Mutex<Vec<AgentRequest>>);
impl ApprovalSink for Cards {
    fn opened(&self, _lease: &str, request: &AgentRequest) {
        self.0.lock().unwrap().push(request.clone());
    }
}

/// A local origin that records every raw request it receives and answers "ok".
async fn origin() -> (u16, Arc<Mutex<Vec<String>>>) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(vec![]));
    let s = seen.clone();
    tokio::spawn(async move {
        loop {
            let (mut c, _) = l.accept().await.unwrap();
            let s = s.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = c.read(&mut buf).await.unwrap_or(0);
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                let echo = text.starts_with("PING");
                s.lock().unwrap().push(text.clone());
                if echo {
                    let _ = c.write_all(b"PONG").await;
                } else {
                    let _ = c
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                        )
                        .await;
                }
                let _ = c.shutdown().await;
            });
        }
    });
    (port, seen)
}

fn auth(lease: &str, token: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(format!("{lease}:{token}"))
}

/// What the sandbox does: one raw request through the proxy, the whole answer back.
async fn send(proxy: u16, raw: String) -> String {
    let mut c = TcpStream::connect(("127.0.0.1", proxy)).await.unwrap();
    c.write_all(raw.as_bytes()).await.unwrap();
    let mut out = vec![];
    c.read_to_end(&mut out).await.unwrap();
    String::from_utf8_lossy(&out).to_string()
}

fn get(proxy_auth: &str, host: &str, port: u16, extra: &str) -> String {
    format!(
        "GET http://{host}:{port}/repos/zoen?x=1 HTTP/1.1\r\nHost: {host}\r\nProxy-Authorization: Basic {proxy_auth}\r\n{extra}\r\n"
    )
}

#[tokio::test]
async fn a_tool_reaches_only_what_it_declared_and_never_sees_the_secret() {
    let (oport, seen) = origin().await;
    let lo = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let resolver = Resolver::Static(HashMap::from([
        ("api.github.test".to_string(), vec![lo]),
        ("docs.example.test".to_string(), vec![lo]),
        ("evil.example.test".to_string(), vec![lo]),
        (
            "rebind.example.test".to_string(),
            vec!["169.254.169.254".parse().unwrap()],
        ),
    ]));
    let cards = Arc::new(Cards::default());
    let egress = Egress::new(
        EgressConfig {
            allow_loopback_upstreams: true,
            ..Default::default()
        },
        resolver,
        Arc::new(Vault),
        cards.clone(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pport = listener.local_addr().unwrap().port();
    tokio::spawn(egress.clone().serve(listener));

    let mut gh = EgressRule::host("api.github.test");
    gh.ports = vec![oport];
    let mut rebind = EgressRule::host("rebind.example.test");
    rebind.ports = vec![oport];
    egress
        .register(LeasePolicy {
            lease: "lease_1".into(),
            token: "t0k".into(),
            owner: "ana".into(),
            agent: "ana-agent".into(),
            tool: "github-issues".into(),
            rules: vec![gh, rebind],
            secrets: vec![SecretBinding {
                name: "github".into(),
                hosts: vec!["api.github.test".into()],
            }],
        })
        .unwrap();
    let a = auth("lease_1", "t0k");
    let bearer = format!("Authorization: Bearer {}\r\n", placeholder("github"));

    // 1. A declared host works, and the real token reaches it in place of the placeholder.
    let r = send(pport, get(&a, "api.github.test", oport, &bearer)).await;
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    {
        let got = seen.lock().unwrap();
        let last = got.last().unwrap();
        assert!(last.contains(&format!("Bearer {TOKEN}")), "{last}");
        assert!(!last.contains("zoen-secret://"));
        assert!(!last.to_ascii_lowercase().contains("proxy-authorization"));
    }

    // 2. An unlisted host becomes an approval card for Ana.
    // docs.example.test isn't listed: first the card.
    let r = send(pport, get(&a, "docs.example.test", oport, "")).await;
    assert!(r.starts_with("HTTP/1.1 403"), "{r}");
    assert!(r.contains("EGRESS_NEEDS_APPROVAL"), "{r}");
    let card = cards.lock_last();
    assert_eq!(card.title, "Permitir acesso a docs.example.test");
    assert_eq!(card.agent, "ana-agent");
    assert!(
        !card.detail.contains("/repos"),
        "cards name the host, never the path"
    );
    assert!(r.contains(&card.id));

    // Asking again doesn't spam a second card.
    let _ = send(pport, get(&a, "docs.example.test", oport, "")).await;
    assert_eq!(cards.0.lock().unwrap().len(), 1);

    // 3. Ana approves for this task: the request now goes through.
    assert!(egress.approve(&card.id, ApprovalScope::Task).is_none());
    let r = send(pport, get(&a, "docs.example.test", oport, "")).await;
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    // ... but the GitHub secret still can't go there.
    let r = send(pport, get(&a, "docs.example.test", oport, &bearer)).await;
    assert!(r.contains("EGRESS_SECRET_NOT_BOUND"), "{r}");
    let r = send(
        pport,
        get(
            &a,
            "api.github.test",
            oport,
            &format!("X-K: {}\r\n", placeholder("aws")),
        ),
    )
    .await;
    assert!(r.contains("EGRESS_UNKNOWN_SECRET"), "{r}");

    // 4. "Always for this tool" returns a Grant to record, and covers the next lease too.
    let _ = send(pport, get(&a, "evil.example.test", oport, "")).await;
    let card2 = cards.lock_last();
    let grant = egress
        .approve(&card2.id, ApprovalScope::AlwaysForTool)
        .unwrap();
    assert_eq!(grant.grantee.as_deref(), Some("ana-agent"));
    assert!(
        matches!(&grant.capability, Capability::Device { capability, .. } if capability == "net:evil.example.test")
    );
    egress
        .register(LeasePolicy {
            lease: "lease_2".into(),
            token: "t2".into(),
            owner: "ana".into(),
            agent: "ana-agent".into(),
            tool: "github-issues".into(),
            rules: vec![],
            secrets: vec![],
        })
        .unwrap();
    let r = send(
        pport,
        get(&auth("lease_2", "t2"), "evil.example.test", oport, ""),
    )
    .await;
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");

    // 5. Private and metadata addresses stay out, even behind a listed name; IP literals and
    //    mail ports are refused; a wrong credential gets nothing.
    let r = send(pport, get(&a, "rebind.example.test", oport, "")).await;
    assert!(r.contains("EGRESS_PRIVATE_ADDRESS"), "{r}");
    let r = send(pport, get(&a, "169.254.169.254", 80, "")).await;
    assert!(r.contains("EGRESS_IP_LITERAL"), "{r}");
    let r = send(
        pport,
        format!("CONNECT smtp.example.test:25 HTTP/1.1\r\nProxy-Authorization: Basic {a}\r\n\r\n"),
    )
    .await;
    assert!(r.contains("EGRESS_BLOCKED_PORT"), "{r}");
    let r = send(
        pport,
        get(&auth("lease_1", "wrong"), "api.github.test", oport, ""),
    )
    .await;
    assert!(r.starts_with("HTTP/1.1 407"), "{r}");

    // 6. CONNECT tunnels carry bytes both ways for a listed host.
    let mut c = TcpStream::connect(("127.0.0.1", pport)).await.unwrap();
    c.write_all(
        format!(
            "CONNECT api.github.test:{oport} HTTP/1.1\r\nProxy-Authorization: Basic {a}\r\n\r\n"
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    let mut head = [0u8; 39];
    c.read_exact(&mut head).await.unwrap();
    assert!(String::from_utf8_lossy(&head).starts_with("HTTP/1.1 200"));
    c.write_all(b"PING").await.unwrap();
    let mut pong = vec![];
    c.read_to_end(&mut pong).await.unwrap();
    assert_eq!(pong, b"PONG");

    // 7. A revoked lease is dead.
    egress.revoke("lease_1");
    let r = send(pport, get(&a, "api.github.test", oport, "")).await;
    assert!(r.starts_with("HTTP/1.1 407"), "{r}");

    // 8. The log: one line per request, metadata only.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let log = egress.log();
    let text = serde_json::to_string(&log).unwrap();
    assert!(!text.contains(TOKEN), "no secret in the log");
    assert!(!text.contains("zoen-secret"), "no placeholder in the log");
    assert!(
        !text.contains("/repos") && !text.contains("x=1"),
        "no path or query in the log"
    );
    assert!(log.iter().any(|e| e.decision == Decision::NeedsApproval));
    assert!(log.iter().any(|e| e.decision == Decision::Allowed
        && e.host.as_deref() == Some("api.github.test")
        && e.bytes_down > 0));
    println!("egress log lines: {}", log.len());
}

trait LastCard {
    fn lock_last(&self) -> AgentRequest;
}
impl LastCard for Arc<Cards> {
    fn lock_last(&self) -> AgentRequest {
        self.0.lock().unwrap().last().cloned().expect("a card")
    }
}
