use base64::{engine::general_purpose::STANDARD, Engine};
use event_log::{Author, Signer};
use protocol::{ClientFrame, Envelope};
use relay_admission::closed::{
    closed_publish,
    relay::{metadata, preview},
    ClosedContext, ClosedProjection, DirectoryTarget,
};
use relay_admission::portable::SessionContext;
use types::{EventBody, Privacy, Role, Seen, SpaceKind};

fn author(seed: u8) -> Author {
    Author::device(
        &Signer::from_secret(&[seed; 32]),
        Signer::from_secret(&[seed + 1; 32]),
    )
}
fn session(who: &Author) -> SessionContext {
    SessionContext {
        authenticated_identity: who.identity.clone(),
        authenticated_device: who.device.clone().unwrap(),
    }
}
fn context(who: &Author, state: Option<ClosedProjection>) -> ClosedContext {
    ClosedContext {
        session: session(who),
        space_id: "sp_relay_preview".into(),
        history: state
            .as_ref()
            .and_then(|s| s.head.clone())
            .into_iter()
            .collect(),
        projection: state,
        target: None,
        invite: None,
        now_ms: "1000".into(),
        prune_ceiling_secs: None,
    }
}
fn frame(who: &Author, cid: &str, state: Option<&ClosedProjection>, body: EventBody) -> Vec<u8> {
    let seen = state.and_then(|s| s.head.as_ref()).map(|head| Seen {
        seq: head.seq.parse().unwrap(),
        hash: head.hash.clone(),
    });
    ClientFrame::Publish {
        env: Envelope::plain(&who.sign_event("sp_relay_preview", cid, 1000, seen, body)),
    }
    .encode()
}
fn genesis(who: &Author) -> Vec<u8> {
    frame(
        who,
        "genesis",
        None,
        EventBody::SpaceCreated {
            title: "Relay preview".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    )
}
fn text(value: &str) -> EventBody {
    EventBody::MessagePosted {
        message: "m_preview".into(),
        text: value.into(),
        attaches: None,
        reply: None,
        forwarded: false,
    }
}

#[test]
fn exact_frame_hash_and_device_binding_are_verified_without_issuing_a_client_capability() {
    let who = author(7);
    let wire = genesis(&who);
    let m = metadata(&wire, &session(&who)).unwrap();
    assert_eq!(m.frame_hash, event_log::sha256_hex(&wire));
    assert_eq!(m.identity, who.identity);
    assert_eq!(m.device, who.device.unwrap());
    let other = author(17);
    assert!(metadata(&wire, &session(&other)).is_err());
}

#[test]
fn prospective_genesis_grant_does_not_mutate_the_supplied_space_state() {
    let who = author(7);
    let ctx = context(&who, None);
    let wire = genesis(&who);
    let before = serde_json::to_value(&ctx).unwrap();
    let p = preview(&wire, &ctx, None).unwrap();
    assert_eq!(p.grants, vec![who.identity]);
    assert_eq!(serde_json::to_value(&ctx).unwrap(), before);
}

#[test]
fn added_identity_and_role_change_have_exact_discovery_grants() {
    let owner = author(7);
    let peer = author(17);
    let state = closed_publish(&genesis(&owner), &context(&owner, None))
        .unwrap()
        .projection;
    let add = frame(
        &owner,
        "add-peer",
        Some(&state),
        EventBody::MemberAdded {
            identity: peer.identity.clone(),
            role: Role::Member,
        },
    );
    let mut ctx = context(&owner, Some(state));
    ctx.target = Some(DirectoryTarget {
        identity: peer.identity.clone(),
        registered: true,
    });
    assert_eq!(
        preview(&add, &ctx, None).unwrap().grants,
        vec![peer.identity.clone()]
    );
    assert!(!ctx
        .projection
        .as_ref()
        .unwrap()
        .members
        .contains_key(&peer.identity));
    let added = closed_publish(&add, &ctx).unwrap();
    let change = frame(
        &owner,
        "promote-peer",
        Some(&added.projection),
        EventBody::MemberAdded {
            identity: peer.identity.clone(),
            role: Role::Admin,
        },
    );
    let mut ctx = context(&owner, Some(added.projection));
    ctx.target = Some(DirectoryTarget {
        identity: peer.identity.clone(),
        registered: true,
    });
    assert_eq!(
        preview(&change, &ctx, None).unwrap().grants,
        vec![peer.identity]
    );
}

#[test]
fn duplicate_grants_nothing_and_still_validates_actual_original_receipt_and_retry_signature() {
    let owner = author(7);
    let wire = genesis(&owner);
    let accepted = closed_publish(&wire, &context(&owner, None)).unwrap();
    let receipt = STANDARD.decode(&accepted.event.bytes_base64).unwrap();
    let ctx = context(&owner, Some(accepted.projection));
    assert!(preview(&wire, &ctx, Some(&receipt))
        .unwrap()
        .grants
        .is_empty());
    assert!(preview(&wire, &ctx, Some(b"not canonical protobuf")).is_err());
    let other = author(17);
    assert!(preview(
        &wire,
        &context(&other, ctx.projection.clone()),
        Some(&receipt)
    )
    .is_err());
}

#[test]
fn ordinary_messages_grant_nothing_and_unknown_directory_targets_are_not_permissive() {
    let owner = author(7);
    let peer = author(17);
    let state = closed_publish(&genesis(&owner), &context(&owner, None))
        .unwrap()
        .projection;
    let message = frame(
        &owner,
        "message",
        Some(&state),
        text("actual signed clear preview"),
    );
    assert!(
        preview(&message, &context(&owner, Some(state.clone())), None)
            .unwrap()
            .grants
            .is_empty()
    );
    let add = frame(
        &owner,
        "unknown-peer",
        Some(&state),
        EventBody::MemberAdded {
            identity: peer.identity.clone(),
            role: Role::Member,
        },
    );
    let mut ctx = context(&owner, Some(state));
    ctx.target = Some(DirectoryTarget {
        identity: peer.identity,
        registered: false,
    });
    assert!(preview(&add, &ctx, None).is_err());
}

#[test]
fn linked_device_cid_retry_grants_nothing_and_preserves_the_original_canonical_device() {
    let owner = author(7);
    let linked = Author::device(
        &Signer::from_secret(&[7; 32]),
        Signer::from_secret(&[9; 32]),
    );
    assert_eq!(linked.identity, owner.identity);
    assert_ne!(linked.device, owner.device);
    let original = closed_publish(&genesis(&owner), &context(&owner, None)).unwrap();
    let receipt = STANDARD.decode(&original.event.bytes_base64).unwrap();
    let retry = genesis(&linked);
    let ctx = context(&linked, Some(original.projection));
    let prospective = preview(&retry, &ctx, Some(&receipt)).unwrap();
    assert!(prospective.grants.is_empty());
    assert_eq!(prospective.metadata.device, linked.device.unwrap());
    let replay: serde_json::Value = serde_json::from_str(
        &relay_admission::closed::closed_replay_json(
            &retry,
            &serde_json::to_string(&ctx.session).unwrap(),
            &serde_json::to_string(&ctx.projection).unwrap(),
            &receipt,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(replay["kind"], "duplicate");
    assert_eq!(replay["event"]["hash"], original.event.hash);
    assert_eq!(replay["event"]["bytes_base64"], original.event.bytes_base64);
}
