//! Journey (ADR 0028 §6): Ana's agent runs a tool that talks to the network. The tool reaches
//! only what its manifest declared; anything else becomes an approval card that Ana answers;
//! her GitHub token is used without ever entering the sandbox; the log holds metadata only.

use base64::Engine;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use types::{AgentRequest, Capability};
use zoen_egress::{
    placeholder, ApprovalScope, ApprovalSink, Decision, Egress, EgressConfig, EgressRule,
    LeasePolicy, Resolver, SecretBinding, SecretSource,
};

type Check<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
type Seen = Arc<Mutex<Vec<String>>>;

const TOKEN: &str = "ghp_REAL_SECRET_never_in_sandbox";

struct Vault;
impl SecretSource for Vault {
    fn secret(&self, owner: &str, name: &str) -> Option<String> {
        (owner == "ana" && name == "github").then(|| TOKEN.to_string())
    }
}

#[derive(Default)]
struct Cards(Mutex<Vec<AgentRequest>>, std::sync::atomic::AtomicBool);
impl ApprovalSink for Cards {
    fn opened(&self, _lease: &str, request: &AgentRequest) {
        match self.0.lock() {
            Ok(mut cards) => cards.push(request.clone()),
            Err(_) => self.1.store(true, std::sync::atomic::Ordering::SeqCst),
        }
    }
}

/// A local origin that records every raw request it receives and answers "ok".
async fn origin() -> std::io::Result<(u16, Seen)> {
    let l = TcpListener::bind("127.0.0.1:0").await?;
    let port = l.local_addr()?.port();
    let seen = Arc::new(Mutex::new(vec![]));
    let s = seen.clone();
    tokio::spawn(async move {
        while let Ok((mut c, _)) = l.accept().await {
            let s = s.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = c.read(&mut buf).await?;
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                let echo = text.starts_with("PING");
                s.lock()
                    .map_err(|error| std::io::Error::other(error.to_string()))?
                    .push(text.clone());
                if echo {
                    c.write_all(b"PONG").await?;
                } else {
                    c.write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .await?;
                }
                c.shutdown().await
            });
        }
    });
    Ok((port, seen))
}

fn auth(lease: &str, token: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(format!("{lease}:{token}"))
}

/// What the sandbox does: one raw request through the proxy, the whole answer back.
async fn send(proxy: u16, raw: String) -> std::io::Result<String> {
    let mut c = TcpStream::connect(("127.0.0.1", proxy)).await?;
    c.write_all(raw.as_bytes()).await?;
    let mut out = vec![];
    c.read_to_end(&mut out).await?;
    Ok(String::from_utf8_lossy(&out).to_string())
}

fn get(proxy_auth: &str, host: &str, port: u16, extra: &str) -> String {
    format!(
        "GET http://{host}:{port}/repos/zoen?x=1 HTTP/1.1\r\nHost: {host}\r\nProxy-Authorization: Basic {proxy_auth}\r\n{extra}\r\n"
    )
}

#[tokio::test]
async fn a_tool_reaches_only_what_it_declared_and_never_sees_the_secret() -> Check<()> {
    let (oport, seen) = origin().await?;
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
    )
    .unwrap();
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
            sign_requests: false,
            browser: false,
        })
        .unwrap();
    let a = auth("lease_1", "t0k");
    let bearer = format!("Authorization: Bearer {}\r\n", placeholder("github"));

    let fixture = Fixture {
        pport,
        oport,
        auth: a,
        bearer,
        egress,
        cards,
        seen,
    };
    declared_and_task_approval(&fixture).await?;
    tool_wide_approval(&fixture).await?;
    forbidden_destinations_and_opaque_tunnel(&fixture).await?;
    revocation_and_metadata_log(&fixture).await?;
    Ok(())
}

trait LastCard {
    fn lock_last(&self) -> Check<AgentRequest>;
}
impl LastCard for Arc<Cards> {
    fn lock_last(&self) -> Check<AgentRequest> {
        if self.1.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("Approval callback failed".into());
        }
        self.0
            .lock()
            .map_err(|error| error.to_string())?
            .last()
            .cloned()
            .ok_or_else(|| "Expected an approval card".into())
    }
}

struct Fixture {
    pport: u16,
    oport: u16,
    auth: String,
    bearer: String,
    egress: Arc<Egress>,
    cards: Arc<Cards>,
    seen: Seen,
}

async fn declared_and_task_approval(f: &Fixture) -> Check<()> {
    let Fixture {
        pport,
        oport,
        auth: a,
        bearer,
        egress,
        cards,
        seen,
        ..
    } = f;
    let pport = *pport;
    let oport = *oport;

    // 1. A declared host works, and the real token reaches it in place of the placeholder.
    let r = send(pport, get(a, "api.github.test", oport, bearer)).await?;
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    {
        let got = seen.lock().map_err(|error| error.to_string())?;
        let last = got.last().ok_or("Expected an upstream request")?;
        assert!(last.contains(&format!("Bearer {TOKEN}")), "{last}");
        assert!(!last.contains("zoen-secret://"));
        assert!(!last.to_ascii_lowercase().contains("proxy-authorization"));
    }

    // 2. An unlisted host becomes an approval card for Ana.
    // docs.example.test isn't listed: first the card.
    let r = send(pport, get(a, "docs.example.test", oport, "")).await?;
    assert!(r.starts_with("HTTP/1.1 403"), "{r}");
    assert!(r.contains("EGRESS_NEEDS_APPROVAL"), "{r}");
    let card = cards.lock_last()?;
    assert_eq!(card.title, "Permitir acesso a docs.example.test");
    assert_eq!(card.agent, "ana-agent");
    assert!(
        !card.detail.contains("/repos"),
        "cards name the host, never the path"
    );
    assert!(r.contains(&card.id));

    // Asking again doesn't spam a second card.
    let _ = send(pport, get(a, "docs.example.test", oport, "")).await?;
    assert_eq!(cards.0.lock().map_err(|error| error.to_string())?.len(), 1);

    // 3. Ana approves for this task: the request now goes through.
    assert!(egress.approve(&card.id, ApprovalScope::Task)?.is_none());
    let r = send(pport, get(a, "docs.example.test", oport, "")).await?;
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");
    // ... but the GitHub secret still can't go there.
    let r = send(pport, get(a, "docs.example.test", oport, bearer)).await?;
    assert!(r.contains("EGRESS_SECRET_NOT_BOUND"), "{r}");
    let r = send(
        pport,
        get(
            a,
            "api.github.test",
            oport,
            &format!("X-K: {}\r\n", placeholder("aws")),
        ),
    )
    .await?;
    assert!(r.contains("EGRESS_UNKNOWN_SECRET"), "{r}");

    Ok(())
}

async fn tool_wide_approval(f: &Fixture) -> Check<()> {
    let Fixture {
        pport,
        oport,
        auth: a,
        egress,
        cards,
        ..
    } = f;
    let pport = *pport;
    let oport = *oport;

    // 4. "Always for this tool" returns a Grant to record, and covers the next lease too.
    let _ = send(pport, get(a, "evil.example.test", oport, "")).await?;
    let tool_card = cards.lock_last()?;
    let grant = egress
        .approve(&tool_card.id, ApprovalScope::AlwaysForTool)
        .map_err(std::io::Error::other)?
        .ok_or("Expected a tool-wide grant")?;
    assert_eq!(grant.grantee.as_deref(), Some("ana-agent"));
    assert!(
        matches!(&grant.capability, Capability::Device { capability, .. } if capability == "net:evil.example.test")
    );
    egress.register(LeasePolicy {
        lease: "lease_2".into(),
        token: "t2".into(),
        owner: "ana".into(),
        agent: "ana-agent".into(),
        tool: "github-issues".into(),
        rules: vec![],
        secrets: vec![],
        sign_requests: false,
        browser: false,
    })?;
    let r = send(
        pport,
        get(&auth("lease_2", "t2"), "evil.example.test", oport, ""),
    )
    .await?;
    assert!(r.starts_with("HTTP/1.1 200"), "{r}");

    Ok(())
}

async fn forbidden_destinations_and_opaque_tunnel(f: &Fixture) -> Check<()> {
    let Fixture {
        pport,
        oport,
        auth: a,
        ..
    } = f;
    let pport = *pport;
    let oport = *oport;

    // 5. Private and metadata addresses stay out, even behind a listed name; IP literals and
    //    mail ports are refused; a wrong credential gets nothing.
    let r = send(pport, get(a, "rebind.example.test", oport, "")).await?;
    assert!(r.contains("EGRESS_PRIVATE_ADDRESS"), "{r}");
    let r = send(pport, get(a, "169.254.169.254", 80, "")).await?;
    assert!(r.contains("EGRESS_IP_LITERAL"), "{r}");
    let r = send(
        pport,
        format!("CONNECT smtp.example.test:25 HTTP/1.1\r\nProxy-Authorization: Basic {a}\r\n\r\n"),
    )
    .await?;
    assert!(r.contains("EGRESS_BLOCKED_PORT"), "{r}");
    let r = send(
        pport,
        get(&auth("lease_1", "wrong"), "api.github.test", oport, ""),
    )
    .await?;
    assert!(r.starts_with("HTTP/1.1 407"), "{r}");

    // 6. CONNECT tunnels carry bytes both ways, untouched, for an allowed host no secret is
    //    bound to (hosts with a bound secret are intercepted instead: journey_https).
    let mut c = TcpStream::connect(("127.0.0.1", pport)).await?;
    c.write_all(
        format!(
            "CONNECT docs.example.test:{oport} HTTP/1.1\r\nProxy-Authorization: Basic {a}\r\n\r\n"
        )
        .as_bytes(),
    )
    .await?;
    let mut head = [0u8; 39];
    c.read_exact(&mut head).await?;
    assert!(String::from_utf8_lossy(&head).starts_with("HTTP/1.1 200"));
    c.write_all(b"PING").await?;
    let mut pong = vec![];
    c.read_to_end(&mut pong).await?;
    assert_eq!(pong, b"PONG");

    Ok(())
}

async fn revocation_and_metadata_log(f: &Fixture) -> Check<()> {
    let Fixture {
        pport,
        oport,
        auth: a,
        egress,
        ..
    } = f;
    let pport = *pport;
    let oport = *oport;

    // 7. A revoked lease is dead.
    egress.revoke("lease_1")?;
    let r = send(pport, get(a, "api.github.test", oport, "")).await?;
    assert!(r.starts_with("HTTP/1.1 407"), "{r}");

    // 8. The log: one line per request, metadata only.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let log = egress.log()?;
    let text = serde_json::to_string(&log)?;
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
    Ok(())
}
