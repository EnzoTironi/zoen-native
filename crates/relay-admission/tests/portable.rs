use base64::{engine::general_purpose::STANDARD, Engine};
use event_log::{chain_hash, Author, Signer};
use protocol::{ClientFrame, Envelope, Sequenced};
use relay_admission::portable::{
    admit_publish, admit_publish_json, parse_u64, verify_publish, FactSnapshot, InviteFacts,
    ObservedHash, SessionContext, TargetFacts, TrustedContext, MAX_CONTEXT_BYTES,
    MAX_ENVELOPE_BYTES, MAX_FRAME_BYTES,
};
use relay_admission::Effect;
use types::{EventBody, Privacy, Role, Seen, SpaceKind, GENESIS_PREV};

const SPACE: &str = "sp_cloudflare_admission_candidate";
const CID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

fn author() -> Author {
    Author::device(
        &Signer::from_secret(&[7; 32]),
        Signer::from_secret(&[9; 32]),
    )
}

fn context(author: &Author) -> TrustedContext {
    TrustedContext {
        authenticated_identity: author.identity.clone(),
        authenticated_device: author
            .device
            .clone()
            .unwrap_or_else(|| author.identity.clone()),
        facts: FactSnapshot {
            space: SPACE.into(),
            author: author.identity.clone(),
            ..Default::default()
        },
    }
}

fn session(context: &TrustedContext) -> SessionContext {
    SessionContext {
        authenticated_identity: context.authenticated_identity.clone(),
        authenticated_device: context.authenticated_device.clone(),
    }
}

fn env(author: &Author, body: EventBody, seen: Option<Seen>) -> Envelope {
    Envelope::plain(&author.sign_event(SPACE, CID, 1000, seen, body))
}

fn frame(env: Envelope) -> Vec<u8> {
    ClientFrame::Publish { env }.encode()
}

fn create() -> EventBody {
    EventBody::SpaceCreated {
        title: "Portable fixture".into(),
        kind: SpaceKind::Group,
        privacy: Privacy::Closed,
    }
}

fn message(text: &str) -> EventBody {
    EventBody::MessagePosted {
        message: "m_fixture".into(),
        text: text.into(),
        attaches: None,
        reply: None,
        forwarded: false,
    }
}

fn existing(author: &Author, seq: u64, role: Option<Role>) -> (TrustedContext, Seen) {
    let mut context = context(author);
    let hash = "1".repeat(64);
    context.facts.head = Some(ObservedHash {
        seq: seq.to_string(),
        hash: hash.clone(),
    });
    context.facts.history = vec![ObservedHash {
        seq: seq.to_string(),
        hash: hash.clone(),
    }];
    context.facts.kind = Some(SpaceKind::Group);
    context.facts.privacy = Some(Privacy::Closed);
    context.facts.author_role = role;
    context.facts.member_count = 3;
    (context, Seen { seq, hash })
}

#[test]
fn deterministic_signed_fixture_uses_the_native_genesis_hash_and_protobuf_bytes() {
    let author = author();
    let input = env(&author, create(), None);
    let original_content = input.content().to_vec();
    let wire_hash = input.wire_hash();
    let accepted = admit_publish(&frame(input), &context(&author)).unwrap();
    assert!(matches!(
        accepted.effect,
        Effect::Create {
            kind: SpaceKind::Group,
            privacy: Privacy::Closed
        }
    ));
    assert_eq!(accepted.event.seq, 0);
    assert_eq!(accepted.event.prev, GENESIS_PREV);
    assert_eq!(
        accepted.event.hash,
        chain_hash(SPACE, 0, GENESIS_PREV, &wire_hash)
    );
    let decoded = Sequenced::decode(&accepted.event.encode()).unwrap();
    assert_eq!(decoded.env.content(), original_content);
    decoded.env.verify().unwrap();
}

#[test]
fn verifies_signatures_device_certificates_and_authenticated_subject_before_admission() {
    let author = author();
    let context = context(&author);
    let good = env(&author, create(), None);
    let mut bad_signature = good.clone();
    bad_signature.sig = "00".repeat(64);
    assert_eq!(
        admit_publish(&frame(bad_signature), &context)
            .unwrap_err()
            .code,
        "bad_signature"
    );
    let mut bad_certificate = good.clone();
    bad_certificate.cert = Some("00".repeat(64));
    assert_eq!(
        admit_publish(&frame(bad_certificate), &context)
            .unwrap_err()
            .code,
        "bad_signature"
    );
    let mut another_device = context.clone();
    another_device.authenticated_device = Signer::from_secret(&[11; 32]).id();
    assert_eq!(
        admit_publish(&frame(good.clone()), &another_device)
            .unwrap_err()
            .code,
        "wrong_author"
    );
    let mut another_identity = context;
    another_identity.authenticated_identity = Signer::from_secret(&[13; 32]).id();
    assert_eq!(
        admit_publish(&frame(good), &another_identity)
            .unwrap_err()
            .code,
        "wrong_author"
    );
}

#[test]
fn facts_cannot_be_borrowed_from_another_space_author_or_target_lookup() {
    let author = author();
    let wire = frame(env(&author, create(), None));
    let mut wrong_space = context(&author);
    wrong_space.facts.space = "sp_someone_else".into();
    assert_eq!(
        admit_publish(&wire, &wrong_space).unwrap_err().code,
        "facts_scope"
    );
    let mut wrong_author = context(&author);
    wrong_author.facts.author = Signer::from_secret(&[13; 32]).id();
    assert_eq!(
        admit_publish(&wire, &wrong_author).unwrap_err().code,
        "facts_scope"
    );
    let (mut member, seen) = existing(&author, 3, Some(Role::Admin));
    let target = Signer::from_secret(&[17; 32]).id();
    let wire = frame(env(
        &author,
        EventBody::MemberAdded {
            identity: target.clone(),
            role: Role::Member,
        },
        Some(seen),
    ));
    member.facts.target = Some(TargetFacts {
        identity: Signer::from_secret(&[19; 32]).id(),
        role: None,
        registered: Some(true),
    });
    assert_eq!(
        admit_publish(&wire, &member).unwrap_err().code,
        "facts_scope"
    );
    member.facts.target.as_mut().unwrap().identity = target;
    member.facts.target.as_mut().unwrap().registered = Some(false);
    assert_eq!(
        admit_publish(&wire, &member).unwrap_err().reason,
        "that person isn't on Zoen yet"
    );
}

#[test]
fn uint64_values_stay_exact_above_javascript_numbers_and_overflow_fails() {
    let author = author();
    let (context, seen) = existing(&author, 9_007_199_254_740_993, Some(Role::Member));
    let wire = frame(env(&author, message("exact cursor"), Some(seen)));
    let json = admit_publish_json(&wire, &serde_json::to_string(&context).unwrap()).unwrap();
    let accepted: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(accepted["event"]["seq"], "9007199254740994");
    assert_eq!(accepted["expected_head"]["seq"], "9007199254740993");
    let (maximum, seen) = existing(&author, u64::MAX, Some(Role::Member));
    assert_eq!(
        admit_publish(&frame(env(&author, message("full"), Some(seen))), &maximum)
            .unwrap_err()
            .code,
        "sequence_exhausted"
    );
    for invalid in ["", "01", "-1", "+1", "1e3", "18446744073709551616"] {
        assert!(parse_u64(invalid).is_err());
    }
    assert_eq!(parse_u64("18446744073709551615").unwrap(), u64::MAX);
}

#[test]
fn numeric_json_cursors_and_unknown_context_fields_are_refused() {
    let author = author();
    let (context, seen) = existing(&author, 3, Some(Role::Member));
    let wire = frame(env(&author, message("bounded context"), Some(seen)));
    let mut numeric = serde_json::to_value(&context).unwrap();
    numeric["facts"]["head"]["seq"] = serde_json::json!(3);
    assert_eq!(
        admit_publish_json(&wire, &numeric.to_string())
            .unwrap_err()
            .code,
        "invalid_facts"
    );
    let mut unknown = serde_json::to_value(&context).unwrap();
    unknown["skip_authorization"] = serde_json::json!(true);
    assert_eq!(
        admit_publish_json(&wire, &unknown.to_string())
            .unwrap_err()
            .code,
        "invalid_facts"
    );
}

#[test]
fn native_frame_envelope_and_context_bounds_apply_before_expensive_work() {
    let author = author();
    let context = context(&author);
    assert_eq!(
        admit_publish(&vec![0; MAX_FRAME_BYTES + 1], &context)
            .unwrap_err()
            .code,
        "too_large"
    );
    assert_eq!(
        admit_publish(&ClientFrame::Ping.encode(), &context)
            .unwrap_err()
            .code,
        "invalid_frame"
    );
    let large = frame(env(&author, message(&"x".repeat(MAX_ENVELOPE_BYTES)), None));
    assert_eq!(
        admit_publish(&large, &context).unwrap_err().code,
        "too_large"
    );
    assert_eq!(
        admit_publish_json(
            &frame(env(&author, create(), None)),
            &"x".repeat(MAX_CONTEXT_BYTES + 1)
        )
        .unwrap_err()
        .code,
        "too_large"
    );
}

#[test]
fn transport_invite_is_not_retained_and_unknown_protobuf_fields_stay_compatible() {
    let author = author();
    let mut input = env(&author, create(), None);
    input.invite = Some("0123456789".into());
    let mut wire = frame(input);
    let first = admit_publish(&wire, &context(&author)).unwrap();
    assert!(first.event.env.invite.is_none());
    wire.extend_from_slice(&[0xf8, 0x03, 0x01]); // Unknown protobuf field 63, varint 1.
    let second = admit_publish(&wire, &context(&author)).unwrap();
    assert_eq!(first.event.encode(), second.event.encode());
    let json =
        admit_publish_json(&wire, &serde_json::to_string(&context(&author)).unwrap()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json).unwrap();
    let bytes = STANDARD
        .decode(json["event"]["bytes_base64"].as_str().unwrap())
        .unwrap();
    assert!(Sequenced::decode(&bytes).unwrap().env.invite.is_none());
}

#[test]
fn duplicate_receipts_must_bypass_new_membership_admission_after_signature_validation() {
    let author = author();
    let (member, seen) = existing(&author, 3, Some(Role::Member));
    let wire = frame(env(&author, message("previously accepted"), Some(seen)));
    admit_publish(&wire, &member).unwrap();
    let mut removed = member.clone();
    removed.facts.author_role = None;
    assert_eq!(
        admit_publish(&wire, &removed).unwrap_err().reason,
        "not a member of this space"
    );
    let verified = verify_publish(&wire, &session(&removed)).unwrap();
    assert_eq!(
        (verified.space(), verified.author(), verified.client_id()),
        (SPACE, author.identity.as_str(), CID)
    );
    // The DO must return its original receipt here, with CURRENT membership for
    // delivery, instead of calling admit_publish again with removed.facts.
}

#[test]
fn root_signed_envelopes_preserve_the_native_optional_device_rule() {
    let author = Author::root(Signer::from_secret(&[23; 32]));
    let mut context = context(&author);
    context.authenticated_device = Signer::from_secret(&[29; 32]).id();
    admit_publish(&frame(env(&author, create(), None)), &context).unwrap();
}

#[test]
fn invite_lookup_keeps_native_uppercase_normalization_for_lowercase_transport() {
    let author = author();
    let (mut context, seen) = existing(&author, 3, None);
    context.facts.target = Some(TargetFacts {
        identity: author.identity.clone(),
        role: None,
        registered: Some(true),
    });
    context.facts.invite = Some(InviteFacts {
        code_hash: relay_admission::code_hash("  ABCDEFGHJK  "),
        space: SPACE.into(),
        role: Role::Member,
    });
    let mut input = env(
        &author,
        EventBody::MemberAdded {
            identity: author.identity.clone(),
            role: Role::Member,
        },
        Some(seen),
    );
    input.invite = Some("abcdefghjk".into());
    let lowercase = admit_publish(&frame(input.clone()), &context).unwrap();
    input.invite = Some("ABCDEFGHJK".into());
    let uppercase = admit_publish(&frame(input), &context).unwrap();
    assert_eq!(lowercase.event.encode(), uppercase.event.encode());
    assert!(matches!(
        lowercase.effect,
        Effect::Add {
            by_invite: true,
            ..
        }
    ));
    context.facts.invite.as_mut().unwrap().code_hash = "0".repeat(64);
    let mut input = env(
        &author,
        EventBody::MemberAdded {
            identity: author.identity.clone(),
            role: Role::Member,
        },
        Some(Seen {
            seq: 3,
            hash: "1".repeat(64),
        }),
    );
    input.invite = Some("abcdefghjk".into());
    assert_eq!(
        admit_publish(&frame(input), &context).unwrap_err().code,
        "facts_scope"
    );
}

#[test]
fn trusted_context_failures_remain_retryable_while_invalid_signed_frames_do_not() {
    let author = author();
    let (mut context, seen) = existing(&author, 3, Some(Role::Member));
    let wire = frame(env(
        &author,
        message("retry after reloading server facts"),
        Some(seen),
    ));
    context.facts.privacy = None;
    let missing = admit_publish(&wire, &context).unwrap_err();
    assert_eq!(missing.code, "invalid_facts");
    assert!(!missing.permanent);
    context.facts.privacy = Some(Privacy::Closed);
    admit_publish(&wire, &context).unwrap();
    context.facts.author = "0".repeat(64);
    assert!(!admit_publish(&wire, &context).unwrap_err().permanent);
    assert!(!parse_u64("01").unwrap_err().permanent);
    assert!(!admit_publish_json(&wire, "{}").unwrap_err().permanent);
    assert!(admit_publish(&[0xff], &context).unwrap_err().permanent);
    let mut damaged = env(&author, create(), None);
    damaged.sig = "00".repeat(64);
    assert!(
        admit_publish(&frame(damaged), &context)
            .unwrap_err()
            .permanent
    );
}
