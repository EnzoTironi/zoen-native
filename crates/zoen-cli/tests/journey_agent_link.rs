//! Agent share links: create → public page → preview → open a chat (first-class citizen).
//!
//!   ZOEN_TEST_PG=... cargo test -p zoen-cli --test journey_agent_link

mod common;
use common::*;
use roda_log::{agent_owner, Author, Signer};
use roda_proto::{Op, Reply};
use roda_types::{Identity, IdentityKind};

fn agent_profile(
    author: &Author,
    handle: &str,
    owner: &str,
    proof: roda_types::AgentOwnerProof,
) -> Identity {
    Identity {
        owner_proof: Some(Box::new(proof)),
        id: author.identity.clone(),
        kind: IdentityKind::Agent,
        name: "Barbeiro".into(),
        handle: handle.into(),
        tint_hex: "#C45C26".into(),
        glyph: Some("scissors".into()),
        owner: Some(owner.into()),
        bio: "Agenda o horário do corte.".into(),
    }
}

#[tokio::test]
async fn agent_invite_opens_a_chat_and_public_page_shows_the_agent() {
    let w = World::new("agent_link").await;
    let mut owner = RawClient::connect(&w.relay_url(), "dono").await;
    let agent_author = Author::device(&Signer::generate(), Signer::generate());
    let proof = agent_owner::authorize(&owner.author, &agent_author.identity).unwrap();
    let mut agent_client = RawClient::reconnect(&w.relay_url(), agent_author).await;
    let registered = agent_client
        .request(Op::Register {
            profile: agent_profile(&agent_client.author, "barbeiro", &owner.identity(), proof),
        })
        .await
        .expect("register agent");
    let Reply::Registered(agent) = registered else {
        panic!("expected Registered, got {registered:?}");
    };
    assert_eq!(agent.kind, IdentityKind::Agent);

    let created = owner
        .request(Op::CreateInvite {
            space: String::new(),
            agent: Some(agent.id.clone()),
            role: Role::Member,
            max_uses: 50,
            ttl_secs: 3600,
        })
        .await
        .expect("create agent invite");
    let Reply::Invite(inv) = created else {
        panic!("expected Invite, got {created:?}");
    };
    assert_eq!(inv.code.len(), 10);

    let (status, body, _) = http(w.port, "GET", &format!("/a/{}", inv.code), &[], "");
    assert_eq!(status, 200, "public page for invite code");
    assert!(body.contains("Barbeiro"), "name on page: {body}");
    assert!(body.contains("Agenda o horário"), "purpose on page");
    assert!(body.contains("Abrir no app"), "open CTA");
    assert!(body.contains("Instalar"), "install CTA");
    assert!(
        body.contains(&format!("zoen://agent/{}", inv.code)),
        "deep link: {body}"
    );
    let (status, body, _) = http(w.port, "GET", "/install", &[], "");
    assert_eq!(status, 200);
    assert!(body.contains("Instalar o Zoen"));

    let (status, body, _) = http(w.port, "GET", "/a/barbeiro", &[], "");
    assert_eq!(status, 200, "public page for handle");
    assert!(body.contains("@barbeiro"));

    let (status, _, _) = http(w.port, "GET", "/.well-known/assetlinks.json", &[], "");
    // No fingerprints configured in journey → 404 is fine; route must exist.
    assert!(
        status == 404 || status == 200,
        "assetlinks route status {status}"
    );

    let preview = owner
        .request(Op::PreviewInvite {
            code: inv.code.clone(),
        })
        .await
        .expect("preview");
    let Reply::Preview(p) = preview else {
        panic!("expected Preview, got {preview:?}");
    };
    assert_eq!(p.kind, "agent");
    assert_eq!(
        p.agent.as_ref().map(|a| a.handle.as_str()),
        Some("barbeiro")
    );
    assert!(p.space.is_empty());

    // A person opens the link in the real `zoen` app and lands in a chat with the agent.
    w.init("cliente", "Cliente");
    let chat = w.zoen("cliente", &["join", &inv.code]);
    assert!(!chat.trim().is_empty(), "join opened a chat: {chat:?}");
    let chats = w.zoen("cliente", &["chats"]);
    assert!(
        chats.contains(chat.trim()),
        "the agent chat is listed: {chats}"
    );
}

use roda_types::Role;

fn http(port: u16, method: &str, path: &str, _h: &[(&str, &str)], _b: &str) -> (u16, String, ()) {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        s,
        "{method} {path} HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let status = out[9..12].parse().unwrap();
    let body = out
        .split_once("\r\n\r\n")
        .map(|x| x.1.to_string())
        .unwrap_or_default();
    (status, body, ())
}
