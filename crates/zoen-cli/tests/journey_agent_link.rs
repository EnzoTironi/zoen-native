//! Agent share links: one stable link per agent → public page → redeem → chat with the
//! agent (first-class citizen); revoke, expiry, max uses and the page's hardening.
//!
//!   ZOEN_TEST_PG=... cargo test -p zoen-cli --test journey_agent_link

mod common;
use std::sync::atomic::{AtomicU32, Ordering};

use common::*;
use roda_log::{agent_owner, Author, Signer};
use roda_proto::{InviteCreated, Op, Reply};
use roda_types::{Identity, IdentityKind, Role};

const FINGERPRINT: &str = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";
const APP_ID: &str = "TEAM123456.xyz.tironi.zoen";
const IP_HEADER: &str = "x-test-client-ip";

#[tokio::test]
async fn agent_links_are_stable_redeemable_revocable_and_the_page_is_hardened() {
    let w = World::with_env(
        "agent_link",
        &[
            ("ZOEN_ANDROID_FINGERPRINTS", FINGERPRINT),
            ("ZOEN_APPLE_APP_IDS", APP_ID),
            ("ZOEN_CLIENT_IP_HEADER", IP_HEADER),
            ("ZOEN_LIMITS", "agent_page_ip=5/m:5"),
        ],
    )
    .await;
    let mut owner = RawClient::connect(&w.relay_url(), "dono").await;
    let mut stranger = RawClient::connect(&w.relay_url(), "estranho").await;
    let barbeiro = register_agent(&w, &owner, "barbeiro", "#C45C26", Some("scissors")).await;

    // One stable link: asking again returns the same live code, whatever the knobs.
    let a = create(&mut owner, &barbeiro, 50, 3600)
        .await
        .expect("create");
    assert_eq!(a.code.len(), 10);
    let again = create(&mut owner, &barbeiro, 2, 60).await.expect("again");
    assert_eq!(again.code, a.code, "one live link per agent");
    let err = create(&mut stranger, &barbeiro, 50, 3600)
        .await
        .unwrap_err();
    assert!(err.contains("owner"), "non-owner can't share: {err}");
    let err = create(&mut owner, &stranger.identity(), 50, 3600)
        .await
        .unwrap_err();
    assert!(err.contains("owner"), "a person isn't an agent: {err}");

    // The public page, with its headers.
    let page = get(&w, &format!("/a/{}", a.code), &[]);
    assert_eq!(page.status, 200, "public page for invite code");
    for text in [
        "Barbeiro",
        "Agenda o horário",
        "@barbeiro",
        "Abrir no app",
        "Instalar",
        "✂️",
    ] {
        assert!(page.body.contains(text), "{text} on page: {}", page.body);
    }
    assert!(page
        .body
        .contains(&format!(r#"href="https://tryzoen.com/a/{}""#, a.code)));
    assert!(page
        .body
        .contains(&format!(r#"data-deep="zoen://agent/{}""#, a.code)));
    assert!(page.body.contains("--tint: #C45C26;"));
    assert!(page.body.contains("visibilitychange") && page.body.contains("pagehide"));
    assert_eq!(page.header("cache-control"), Some("no-store"));
    assert_eq!(page.header("referrer-policy"), Some("no-referrer"));
    assert_eq!(page.header("x-robots-tag"), Some("noindex"));
    let csp = page.header("content-security-policy").expect("csp");
    assert!(csp.contains("default-src 'none'"), "{csp}");
    assert!(
        csp.contains("script-src 'sha256-"),
        "script allowed by hash only: {csp}"
    );
    assert!(!csp.contains("unsafe-inline"), "{csp}");

    let page = get(&w, "/a/barbeiro", &[]);
    assert_eq!(page.status, 200, "public page for handle");
    assert!(page.body.contains("zoen://agent/barbeiro"));

    // Unknown code or handle, and a person's handle: no page.
    for path in ["/a/ABCDEFGHJK", "/a/ninguem", "/a/dono", "/a/%3Cscript%3E"] {
        let r = get(&w, path, &[]);
        assert_eq!(r.status, 404, "{path}");
        assert_eq!(r.header("cache-control"), Some("no-store"), "{path}");
    }

    // Per-IP rate limit on the page.
    let one_ip = [(IP_HEADER, "203.0.113.9")];
    for _ in 0..5 {
        assert_eq!(get_as(&w, "/a/barbeiro", &one_ip).status, 200);
    }
    let limited = get_as(&w, "/a/barbeiro", &one_ip);
    assert_eq!(limited.status, 429);
    assert!(limited.header("retry-after").is_some());

    let Reply::Preview(p) = owner
        .request(Op::PreviewInvite {
            code: a.code.clone(),
        })
        .await
        .expect("preview")
    else {
        panic!("expected Preview");
    };
    assert_eq!(p.kind, "agent");
    assert_eq!(
        p.agent.as_ref().map(|a| a.handle.as_str()),
        Some("barbeiro")
    );
    assert!(p.space.is_empty());

    // A person opens the link in the real `zoen` app and lands in a chat with the agent;
    // the handle and the code reach the same chat through `open_agent_link`.
    w.init("cliente", "Cliente");
    let chat = w.zoen("cliente", &["join", &a.code]);
    let chat = chat.trim();
    assert!(!chat.is_empty(), "join opened a chat");
    assert!(
        w.zoen("cliente", &["chats"]).contains(chat),
        "the agent chat is listed"
    );
    assert_eq!(w.zoen("cliente", &["agent", "@barbeiro"]).trim(), chat);
    assert_eq!(
        w.zoen(
            "cliente",
            &["agent", &format!("https://tryzoen.com/a/{}", a.code)]
        )
        .trim(),
        chat
    );
    assert!(w.try_zoen("cliente", &["agent", "ninguem"]).is_err());
    assert!(
        w.try_zoen("cliente", &["agent", "dono"]).is_err(),
        "a person isn't an agent"
    );

    // Revoke: only the owner; the old code stops working and a new one is minted.
    let err = revoke(&mut stranger, &barbeiro).await.unwrap_err();
    assert!(err.contains("owner"), "{err}");
    revoke(&mut owner, &barbeiro).await.expect("revoke");
    assert!(owner
        .request(Op::PreviewInvite {
            code: a.code.clone()
        })
        .await
        .is_err());
    assert_eq!(get(&w, &format!("/a/{}", a.code), &[]).status, 404);
    let b = create(&mut owner, &barbeiro, 50, 3600)
        .await
        .expect("new link");
    assert_ne!(b.code, a.code);

    // max_uses is enforced by the relay's atomic redeem; a spent link is replaced.
    let agenda = register_agent(
        &w,
        &owner,
        "agenda",
        "red;}</style><script>alert(1)</script>",
        None,
    )
    .await;
    let c = create(&mut owner, &agenda, 1, 3600).await.expect("create");
    let page = get(&w, &format!("/a/{}", c.code), &[]);
    assert_eq!(page.status, 200);
    assert!(
        page.body.contains("--tint: #4F7CFF;"),
        "injected tint falls back"
    );
    assert!(!page.body.contains("alert(1)"), "{}", page.body);
    assert!(page.body.contains(">A</div>"), "no glyph: the initial");
    redeem(&mut stranger, &c.code).await.expect("first use");
    let err = redeem(&mut stranger, &c.code).await.unwrap_err();
    assert!(err.contains("expired or already used"), "{err}");
    assert_eq!(get(&w, &format!("/a/{}", c.code), &[]).status, 404);
    let d = create(&mut owner, &agenda, 1, 3600)
        .await
        .expect("replaced");
    assert_ne!(d.code, c.code);

    // An expired link: no preview, no page, no redeem.
    revoke(&mut owner, &agenda).await.expect("revoke");
    let e = create(&mut owner, &agenda, 10, 1)
        .await
        .expect("short link");
    tokio::time::sleep(std::time::Duration::from_millis(2200)).await;
    assert!(owner
        .request(Op::PreviewInvite {
            code: e.code.clone()
        })
        .await
        .is_err());
    assert!(redeem(&mut stranger, &e.code).await.is_err());
    assert_eq!(get(&w, &format!("/a/{}", e.code), &[]).status, 404);

    // Install: no store listing yet, so Android gets the same "em breve" page, not a 404.
    for ua in [
        "Mozilla/5.0 (Linux; Android 15) Mobile",
        "Mozilla/5.0 (iPhone; CPU iPhone OS 26_0 like Mac OS X)",
    ] {
        let r = get(&w, "/install", &[("user-agent", ua)]);
        assert_eq!(r.status, 200, "{ua}");
        assert!(r.body.contains("Em breve"), "{ua}: {}", r.body);
        assert!(!r.body.contains("play.google.com"), "{ua}");
    }

    // Universal links and Digital Asset Links.
    let aasa = get(&w, "/.well-known/apple-app-site-association", &[]);
    assert_eq!(aasa.status, 200);
    let aasa: serde_json::Value = serde_json::from_str(&aasa.body).unwrap();
    let details = &aasa["applinks"]["details"][0];
    assert_eq!(details["appIDs"][0], APP_ID);
    assert!(
        details["components"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["/"] == "/a/*"),
        "{aasa}"
    );
    let links = get(&w, "/.well-known/assetlinks.json", &[]);
    assert_eq!(links.status, 200);
    let links: serde_json::Value = serde_json::from_str(&links.body).unwrap();
    assert_eq!(
        links,
        serde_json::json!([{
            "relation": ["delegate_permission/common.handle_all_urls"],
            "target": {
                "namespace": "android_app",
                "package_name": "xyz.tironi.zoen",
                "sha256_cert_fingerprints": [FINGERPRINT],
            }
        }])
    );
}

async fn register_agent(
    w: &World,
    owner: &RawClient,
    handle: &str,
    tint: &str,
    glyph: Option<&str>,
) -> String {
    let author = Author::device(&Signer::generate(), Signer::generate());
    let proof = agent_owner::authorize(&owner.author, &author.identity).unwrap();
    let mut client = RawClient::reconnect(&w.relay_url(), author).await;
    let profile = Identity {
        owner_proof: Some(Box::new(proof)),
        id: client.author.identity.clone(),
        kind: IdentityKind::Agent,
        name: format!("{}{}", handle[..1].to_uppercase(), &handle[1..]),
        handle: handle.into(),
        tint_hex: tint.into(),
        glyph: glyph.map(Into::into),
        owner: Some(owner.identity()),
        bio: if handle == "barbeiro" {
            "Agenda o horário do corte.".into()
        } else {
            String::new()
        },
    };
    match client.request(Op::Register { profile }).await {
        Ok(Reply::Registered(a)) => {
            assert_eq!(a.kind, IdentityKind::Agent);
            a.id
        }
        other => panic!("register agent {handle}: {other:?}"),
    }
}

async fn create(
    c: &mut RawClient,
    agent: &str,
    max_uses: u32,
    ttl_secs: u64,
) -> Result<InviteCreated, String> {
    match c
        .request(Op::CreateInvite {
            space: String::new(),
            agent: Some(agent.into()),
            role: Role::Member,
            max_uses,
            ttl_secs,
        })
        .await?
    {
        Reply::Invite(i) => Ok(i),
        other => panic!("expected Invite, got {other:?}"),
    }
}

async fn redeem(c: &mut RawClient, code: &str) -> Result<(), String> {
    match c
        .request(Op::RedeemAgentInvite { code: code.into() })
        .await?
    {
        Reply::Preview(p) => {
            assert_eq!(p.kind, "agent");
            Ok(())
        }
        other => panic!("expected Preview, got {other:?}"),
    }
}

async fn revoke(c: &mut RawClient, agent: &str) -> Result<(), String> {
    c.request(Op::RevokeAgentInvite {
        agent: agent.into(),
    })
    .await
    .map(drop)
}

struct Resp {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Resp {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A GET from its own client IP, so the per-IP page limit only bites where a test means it.
fn get(w: &World, path: &str, headers: &[(&str, &str)]) -> Resp {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let ip = format!("198.51.100.{}", NEXT.fetch_add(1, Ordering::Relaxed) % 250);
    let mut all = vec![(IP_HEADER, ip.as_str())];
    all.extend_from_slice(headers);
    get_as(w, path, &all)
}

/// A raw HTTP/1.0 GET: the relay closes the connection, so the body is everything after
/// the headers.
fn get_as(w: &World, path: &str, headers: &[(&str, &str)]) -> Resp {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(("127.0.0.1", w.port)).unwrap();
    let extra: String = headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}\r\n"))
        .collect();
    write!(s, "GET {path} HTTP/1.0\r\nHost: 127.0.0.1\r\n{extra}\r\n").unwrap();
    let mut raw = String::new();
    s.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let mut lines = head.lines();
    let status = lines.next().unwrap()[9..12].parse().unwrap();
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    Resp {
        status,
        headers,
        body: body.to_string(),
    }
}
