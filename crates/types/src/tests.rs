#[test]
fn ids_sort_by_creation_and_carry_their_time() {
    let ids: Vec<String> = (0..2000).map(|_| new_id("sp")).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "same-millisecond ids keep creation order");
    assert!(ids.iter().all(|i| i.len() == 29 && i.starts_with("sp_")));
    let t = id_time_ms(&ids[0]).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    assert!((now - t).abs() < 5_000);
    assert_eq!(id_time_ms(&format!("sp_{}", "ab".repeat(16))), None);
    let before = new_ulid(now);
    assert!(
        new_ulid(now - 60_000) > before,
        "a clock step back still sorts later"
    );
    assert_eq!(
        id_time_ms(&new_ulid(1_700_000_000_123)).map(|t| t >= 1_700_000_000_123),
        Some(true)
    );
}

use super::*;

#[test]
fn plan_total_and_line_ops() {
    let mut p = PlanDoc {
        title: "Paraty".into(),
        summary: String::new(),
        budget_cents: Some(150_000),
        sections: vec![PlanSection {
            title: "Hospedagem".into(),
            lines: vec![
                PlanLine {
                    id: "a".into(),
                    text: "Pousada".into(),
                    cost_cents: 42_000,
                    done: false,
                },
                PlanLine {
                    id: "b".into(),
                    text: "Taxa".into(),
                    cost_cents: 1_000,
                    done: false,
                },
            ],
        }],
    };
    assert_eq!(p.total_cents(), 43_000);
    p.line_mut("b").unwrap().cost_cents = 2_000;
    assert_eq!(p.total_cents(), 44_000);
    assert!(p.remove_line("a").is_some());
    assert_eq!(p.total_cents(), 2_000);
    assert!(p.line("a").is_none());
}

#[test]
fn ulids_sort_by_time_and_are_unique() {
    let a = new_ulid(1_000);
    let b = new_ulid(2_000);
    assert_eq!(a.len(), 26);
    assert!(a < b);
    assert_ne!(new_ulid(5), new_ulid(5));
}

#[test]
fn ids_are_unique_and_prefixed() {
    let a = new_id("sp");
    let b = new_id("sp");
    assert_ne!(a, b);
    assert!(a.starts_with("sp_"));
    assert_eq!(a.len(), 3 + 26);
}

#[test]
fn event_body_roundtrips_through_json() {
    let body = EventBody::RequestOpened {
        request: AgentRequest {
            tool_proposal: None,
            id: "rq_1".into(),
            agent: "ab".into(),
            title: "Reservar".into(),
            detail: "2 noites".into(),
            audience: "Pousada".into(),
            action: ActionClass::Money { cents: 42_000 },
            content_hash: "h".into(),
            item: None,
            line: None,
            proposal: None,
        },
    };
    let json = serde_json::to_string(&body).unwrap();
    let back: EventBody = serde_json::from_str(&json).unwrap();
    assert_eq!(body, back);
}

#[test]
fn historical_identity_bytes_are_unchanged_without_an_owner_proof() {
    let legacy = r##"{"id":"agent","kind":"Agent","name":"Agent","handle":"agent","tint_hex":"#fff","glyph":null,"owner":"person","bio":""}"##;
    let profile: Identity = serde_json::from_str(legacy).unwrap();
    assert!(profile.owner_proof.is_none());
    assert_eq!(serde_json::to_string(&profile).unwrap(), legacy);
}

#[test]
fn historical_request_and_resolution_bytes_are_preserved() {
    for bytes in [
        r#"{"RequestOpened":{"request":{"id":"rq_1","agent":"ab","title":"Reservar","detail":"2 noites","audience":"Pousada","action":{"Money":{"cents":42000}},"content_hash":"h","item":null,"line":null}}}"#,
        r#"{"RequestResolved":{"request":"rq_1","approved":true,"content_hash":"h"}}"#,
    ] {
        let event: EventBody = serde_json::from_str(bytes).unwrap();
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            bytes,
            "new optional fields must not alter historical signed body bytes"
        );
    }
}

#[test]
fn historical_file_bytes_are_preserved_without_voice_metadata() {
    let bytes = r#"{"name":"song.mp3","mime":"audio/mpeg","bytes":3,"sha256":"abc","chunks":[]}"#;
    let mut file: FileDoc = serde_json::from_str(bytes).unwrap();
    assert!(file.voice.is_none());
    assert!(!file.is_voice());
    assert_eq!(serde_json::to_string(&file).unwrap(), bytes);
    file.voice = Some(VoiceMeta::default());
    assert!(file.is_voice());
    file.mime = "image/png".into();
    assert!(
        !file.is_voice(),
        "a tag cannot turn a non-audio file into audio"
    );
    let old_meta = r#"{"ms":1200,"levels":"08f","transcript":"Original"}"#;
    let meta: VoiceMeta = serde_json::from_str(old_meta).unwrap();
    assert!(meta.reply.is_none());
    assert_eq!(serde_json::to_string(&meta).unwrap(), old_meta);
}

#[test]
fn voice_waveforms_are_bounded_on_send_and_receive() {
    let meta = VoiceMeta {
        levels: "f".repeat(1000),
        ..VoiceMeta::default()
    };
    assert_eq!(meta.bars(), vec![1.0; 160]);
    assert_eq!(VoiceMeta::encode(&vec![0.5; 1000]).len(), 160);
    assert_eq!(VoiceMeta::encode(&[f32::NAN, -1.0, 2.0]), "00f");
}
