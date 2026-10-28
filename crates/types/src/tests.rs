#[test]
fn ids_sort_by_creation_and_carry_their_time() {
    let ids: Vec<String> = (0..2000).map(|_| new_id("sp").unwrap()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "same-millisecond ids keep creation order");
    assert!(ids.iter().all(|i| i.len() == 29 && i.starts_with("sp_")));
    let t = id_time_ms(&ids[0]).unwrap();
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    assert!((now - t).abs() < 5_000);
    assert_eq!(id_time_ms(&format!("sp_{}", "ab".repeat(16))), None);
    let before = new_ulid(now).unwrap();
    assert!(
        new_ulid(now - 60_000).unwrap() > before,
        "a clock step back still sorts later"
    );
    assert_eq!(
        id_time_ms(&new_ulid(1_700_000_000_123).unwrap()).map(|t| t >= 1_700_000_000_123),
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
    let a = new_ulid(1_000).unwrap();
    let b = new_ulid(2_000).unwrap();
    assert_eq!(a.len(), 26);
    assert!(a < b);
    assert_ne!(new_ulid(5).unwrap(), new_ulid(5).unwrap());
}

#[test]
fn ids_are_unique_and_prefixed() {
    let a = new_id("sp").unwrap();
    let b = new_id("sp").unwrap();
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

#[test]
fn legacy_schema1_and_schema2_tool_proposal_bytes_remain_exact_without_review_lineage() {
    for schema in [1, 2] {
        let bytes = format!(
            r#"{{"schema":{schema},"source":"source","serving":"person","source_sha256":"sha","device":"device","owner":"owner","task_sha256":"task","step":0,"artifact":"item","version":1,"sha256":"hash","canonical_tool":"read","arguments_json":"{{}}","worker_binding":"worker","engine_revision":"revision","continuation_json":"{{}}","expires_at_ms":100,"gate_rule":"rule"}}"#
        );
        let mut proposal: ToolProposal = serde_json::from_str(&bytes).unwrap();
        assert!(proposal.review.is_none());
        assert_eq!(serde_json::to_string(&proposal).unwrap(), bytes);
        proposal.review = Some(ToolReviewLineage {
            root: "root".into(),
            predecessor: "previous".into(),
            intent: "signed-owner-intent".into(),
            generation: 1,
        });
        assert_eq!(
            serde_json::from_str::<ToolProposal>(&serde_json::to_string(&proposal).unwrap())
                .unwrap(),
            proposal
        );
        let mut invalid = serde_json::to_value(proposal).unwrap();
        invalid["review"]["approves"] = true.into();
        assert!(serde_json::from_value::<ToolProposal>(invalid).is_err());
    }
}

#[test]
fn voice_waveform_quantization_preserves_each_digit_and_nonfinite_samples() {
    for digit in 0_u8..=15 {
        let level = f32::from(digit) / 15.0;
        let expected = char::from(b"0123456789abcdef"[usize::from(digit)]).to_string();
        assert_eq!(VoiceMeta::encode(&[level]), expected);
        let meta = VoiceMeta {
            levels: expected,
            ..VoiceMeta::default()
        };
        assert!((meta.bars()[0] - level).abs() < f32::EPSILON);
    }
    assert_eq!(
        VoiceMeta::encode(&[f32::NAN, f32::NEG_INFINITY, f32::INFINITY]),
        "00f"
    );
    assert_eq!(VoiceMeta::encode(&[0.499, 0.5, 0.501]), "788");
    let meta = VoiceMeta {
        levels: "👋g8".into(),
        ..VoiceMeta::default()
    };
    assert_eq!(meta.bars().len(), 1);
    assert!((meta.bars()[0] - 8.0 / 15.0).abs() < f32::EPSILON);
}

#[test]
fn item_proposal_hash_preserves_legacy_domain_bytes_and_refuses_absent_output() {
    let mut request: AgentRequest = serde_json::from_str(r#"{"id":"rq_fixed","agent":"agent","title":"Create","detail":"Exact","audience":"Private","action":"Reversible","content_hash":"","item":null,"line":null,"proposal":{"item":"it_fixed","kind":"Note","content":{"Text":{"text":"ação 👋"}},"origin":"reviewed","message":"Created","ai_cost_cents":42}}"#).unwrap();
    assert_eq!(
        request.proposal_hash("sp_fixed").as_deref(),
        Some("490c0593e8c725cada758a45947fa0571e45117232fc074bab4ebc97affcaf7f")
    );
    assert_ne!(
        request.proposal_hash("sp_other"),
        request.proposal_hash("sp_fixed")
    );
    request.proposal.as_mut().unwrap().message.push('!');
    assert_ne!(
        request.proposal_hash("sp_fixed").as_deref(),
        Some("490c0593e8c725cada758a45947fa0571e45117232fc074bab4ebc97affcaf7f")
    );
    request.proposal = None;
    assert_eq!(request.proposal_hash("sp_fixed"), None);
}

#[test]
fn manual_unread_wire_omits_false_and_preserves_true() -> Result<(), serde_json::Error> {
    let implicit: ChatPrefs = serde_json::from_str(r#"{"pinned":false,"archived":false}"#)?;
    assert!(!implicit.manual_unread);
    assert_eq!(
        serde_json::to_string(&implicit)?,
        r#"{"pinned":false,"archived":false}"#
    );
    let explicit = ChatPrefs {
        manual_unread: true,
        ..implicit
    };
    let bytes = serde_json::to_string(&explicit)?;
    assert_eq!(
        bytes,
        r#"{"manual_unread":true,"pinned":false,"archived":false}"#
    );
    assert_eq!(serde_json::from_str::<ChatPrefs>(&bytes)?, explicit);
    Ok(())
}
