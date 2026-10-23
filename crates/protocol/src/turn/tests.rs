use super::*;

#[test]
fn turn_uri_grammar_refuses_credentials_paths_fragments_and_stun_fallbacks() {
    for url in [
        "turns:turn.example.test:5349?transport=tcp",
        "turn:turn.example.test:3478?transport=udp",
        "turns:[2001:db8::1]:5349",
        "turns:localhost",
    ] {
        assert!(valid_url(url), "{url}");
    }
    for url in [
        "stun:public.example.test",
        "https://turn.example.test",
        "turns://host",
        "turns:user@host",
        "turns:host/path",
        "turns:host#secret",
        "turns:host?x=y",
        "turns:host?transport=udp",
        "turns:host:0",
        "turns:host:65536",
        "turns:",
        "turns:[invalid]:5349",
        "turns:host\n",
        "turns:host:1:2",
        "turns:-bad.host",
    ] {
        assert!(!valid_url(url), "{url}");
    }
    assert!(!valid_urls(&["turn:host:3478".into()]));
    assert!(!valid_urls(&vec!["turns:host:5349".into(); MAX_URLS + 1]));
}

#[test]
fn new_wire_tags_round_trip_and_debug_never_discloses_turn_password() {
    use crate::{ClientFrame, Op, Reply, ServerFrame};
    let request = ClientFrame::Req {
        id: 7,
        op: Op::CallTurn {
            space: "private-dm".into(),
            peer: "ab".repeat(32),
        },
    };
    assert_eq!(ClientFrame::decode(&request.encode()).unwrap(), request);
    let credentials = TurnCredentials {
        urls: vec!["turns:turn.example.test:5349".into()],
        username: format!("1700000100:{}", "ab".repeat(32)),
        credential: "AAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
        expires_at_ms: 1_700_000_100_000,
    };
    let response = ServerFrame::Res {
        id: 7,
        result: Ok(Reply::CallTurn(credentials.clone())),
    };
    assert_eq!(ServerFrame::decode(&response.encode()).unwrap(), response);
    let debug = format!("{response:?}");
    assert!(!debug.contains(&credentials.credential) && !debug.contains(&credentials.username));
    let mut invalid = credentials;
    invalid.urls = vec!["stun:public.example.test".into()];
    let response = ServerFrame::Res {
        id: 7,
        result: Ok(Reply::CallTurn(invalid)),
    };
    assert!(ServerFrame::decode(&response.encode()).is_err());
}
