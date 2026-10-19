use base64::{engine::general_purpose::STANDARD, Engine};
use event_log::{Author, Signer};
use protocol::{ClientFrame, Envelope};
use relay_admission::closed::{
    closed_invite_json, closed_publish, closed_publish_json, closed_replay_json, ClosedContext,
    ClosedInvite, ClosedProjection, DirectoryTarget,
};
use relay_admission::invite::{authorize_invite_kind, authorize_inviter, invite_limits};
use relay_admission::portable::{verify_publish_json, SessionContext};
use types::{EventBody, Privacy, Role, Seen, SpaceKind};

const SPACE: &str = "sp_closed_rust_projection";
fn author(seed: u8) -> Author {
    Author::device(
        &Signer::from_secret(&[seed; 32]),
        Signer::from_secret(&[seed + 1; 32]),
    )
}
fn session(author: &Author) -> SessionContext {
    SessionContext {
        authenticated_identity: author.identity.clone(),
        authenticated_device: author.device.clone().unwrap(),
    }
}
fn context(author: &Author, projection: Option<ClosedProjection>) -> ClosedContext {
    let history = projection
        .as_ref()
        .and_then(|p| p.head.clone())
        .into_iter()
        .collect();
    ClosedContext {
        session: session(author),
        projection,
        space_id: SPACE.into(),
        history,
        target: None,
        invite: None,
        now_ms: "1000".into(),
        prune_ceiling_secs: None,
    }
}
fn signed(
    author: &Author,
    cid: &str,
    state: Option<&ClosedProjection>,
    body: EventBody,
    invite: Option<&str>,
) -> Vec<u8> {
    let seen = state.and_then(|p| p.head.as_ref()).map(|h| Seen {
        seq: h.seq.parse().unwrap(),
        hash: h.hash.clone(),
    });
    let mut env = Envelope::plain(&author.sign_event(SPACE, cid, 1000, seen, body));
    env.invite = invite.map(str::to_string);
    ClientFrame::Publish { env }.encode()
}
fn create(privacy: Privacy) -> EventBody {
    EventBody::SpaceCreated {
        title: "Closed fixture".into(),
        kind: SpaceKind::Group,
        privacy,
    }
}
fn message(text: &str) -> EventBody {
    EventBody::MessagePosted {
        message: "m_test".into(),
        text: text.into(),
        attaches: None,
        reply: None,
        forwarded: false,
    }
}
fn initial(owner: &Author) -> ClosedProjection {
    closed_publish(
        &signed(owner, "create", None, create(Privacy::Closed), None),
        &context(owner, None),
    )
    .unwrap()
    .projection
}

#[test]
fn signed_member_removal_persists_its_sequence_and_signed_rejoin_clears_it_atomically() {
    use relay_admission::closed::reads::plan;
    let owner = author(7);
    let peer = author(17);
    let state = add(&owner, initial(&owner), &peer, Role::Member);
    let removal = signed(
        &owner,
        "remove-read-peer",
        Some(&state),
        EventBody::MemberRemoved {
            identity: peer.identity.clone(),
        },
        None,
    );
    let removed = closed_publish(&removal, &context(&owner, Some(state))).unwrap();
    let last: u64 = removed.event.seq.parse().unwrap();
    assert_eq!(
        removed.projection.removed_at.get(&peer.identity),
        Some(&removed.event.seq)
    );
    assert_eq!(
        plan(&removed.projection, &session(&peer), last, 256)
            .unwrap()
            .through
            .as_deref(),
        Some(removed.event.seq.as_str())
    );
    assert!(plan(&removed.projection, &session(&peer), last + 1, 256)
        .unwrap()
        .through
        .is_none());
    let rejoined = add(&owner, removed.projection, &peer, Role::Member);
    assert!(!rejoined.removed_at.contains_key(&peer.identity));
    assert!(plan(&rejoined, &session(&peer), last + 1, 256)
        .unwrap()
        .through
        .is_some());
}
fn add(owner: &Author, state: ClosedProjection, target: &Author, role: Role) -> ClosedProjection {
    let wire = signed(
        owner,
        &format!("add-{}", target.identity),
        Some(&state),
        EventBody::MemberAdded {
            identity: target.identity.clone(),
            role,
        },
        None,
    );
    let mut ctx = context(owner, Some(state));
    ctx.target = Some(DirectoryTarget {
        identity: target.identity.clone(),
        registered: true,
    });
    closed_publish(&wire, &ctx).unwrap().projection
}
fn mint(
    owner: &Author,
    state: &ClosedProjection,
    code: &str,
    role: Role,
    max: u32,
) -> ClosedInvite {
    serde_json::from_str(&closed_invite_json(&serde_json::to_string(state).unwrap(), &serde_json::json!({
        "who":owner.identity,"code":code,"role":role,"max_uses":max,"ttl_secs":"60","now_ms":"1000","existing":null
    }).to_string()).unwrap()).unwrap()
}

#[test]
fn rust_projection_owns_genesis_membership_and_exact_canonical_result() {
    let owner = author(7);
    let accepted = closed_publish(
        &signed(&owner, "create", None, create(Privacy::Closed), None),
        &context(&owner, None),
    )
    .unwrap();
    assert_eq!(accepted.event.seq, "0");
    assert_eq!(
        accepted.projection.members.get(&owner.identity),
        Some(&Role::Owner)
    );
    assert_eq!(accepted.audience, vec![owner.identity]);
    let encoded = serde_json::to_string(&accepted).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&encoded).unwrap()["event"]["bytes_base64"],
        accepted.event.bytes_base64
    );
}

#[test]
fn removed_author_replay_returns_original_bytes_and_current_audience_even_if_retry_changes() {
    let owner = author(7);
    let member = author(17);
    let state = add(&owner, initial(&owner), &member, Role::Member);
    let original_frame = signed(&member, "same", Some(&state), message("original"), None);
    let original = closed_publish(&original_frame, &context(&member, Some(state))).unwrap();
    let removal = signed(
        &owner,
        "remove",
        Some(&original.projection),
        EventBody::MemberRemoved {
            identity: member.identity.clone(),
        },
        None,
    );
    let removed = closed_publish(
        &removal,
        &context(&owner, Some(original.projection.clone())),
    )
    .unwrap();
    assert!(removed.audience.contains(&member.identity));
    let retry = signed(
        &member,
        "same",
        Some(&removed.projection),
        message("different valid signed bytes"),
        None,
    );
    assert_eq!(
        closed_publish(&retry, &context(&member, Some(removed.projection.clone())))
            .unwrap_err()
            .reason,
        "not a member of this space"
    );
    let replay: serde_json::Value = serde_json::from_str(
        &closed_replay_json(
            &retry,
            &serde_json::to_string(&session(&member)).unwrap(),
            &serde_json::to_string(&removed.projection).unwrap(),
            &STANDARD.decode(&original.event.bytes_base64).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(replay["event"]["bytes_base64"], original.event.bytes_base64);
    assert_eq!(replay["audience"], serde_json::json!([owner.identity]));
    assert!(replay["joined"].is_null());
}

#[test]
fn replay_of_join_reports_current_membership_and_legacy_roster_never_fanouts() {
    let owner = author(7);
    let member = author(17);
    let state = initial(&owner);
    let wire = signed(
        &owner,
        "join",
        Some(&state),
        EventBody::MemberAdded {
            identity: member.identity.clone(),
            role: Role::Member,
        },
        None,
    );
    let mut ctx = context(&owner, Some(state));
    ctx.target = Some(DirectoryTarget {
        identity: member.identity.clone(),
        registered: true,
    });
    let accepted = closed_publish(&wire, &ctx).unwrap();
    let mut projection = accepted.projection;
    let original = STANDARD.decode(&accepted.event.bytes_base64).unwrap();
    let call = |projection: &ClosedProjection| {
        serde_json::from_str::<serde_json::Value>(
            &closed_replay_json(
                &wire,
                &serde_json::to_string(&session(&owner)).unwrap(),
                &serde_json::to_string(projection).unwrap(),
                &original,
            )
            .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(call(&projection)["joined"], member.identity);
    projection
        .members
        .insert("legacy-invalid-identity".into(), Role::Member);
    let replay = call(&projection);
    assert_eq!(replay["audience"], serde_json::json!([]));
    assert!(replay["joined"].is_null());
}

#[test]
fn lowercase_invite_expiry_and_usage_are_applied_only_after_successful_admission() {
    let owner = author(7);
    let joining = author(17);
    let next = author(27);
    let state = initial(&owner);
    let invite = mint(&owner, &state, "ABCDEFGHJK", Role::Member, 1);
    let wire = signed(
        &joining,
        "self-join",
        None,
        EventBody::MemberAdded {
            identity: joining.identity.clone(),
            role: Role::Member,
        },
        Some("abcdefghjk"),
    );
    let mut ctx = context(&joining, Some(state.clone()));
    ctx.invite = Some(invite.clone());
    ctx.target = Some(DirectoryTarget {
        identity: joining.identity.clone(),
        registered: true,
    });
    let accepted = closed_publish(&wire, &ctx).unwrap();
    assert_eq!(accepted.invite_update.as_ref().unwrap().uses, 1);
    assert_eq!(invite.uses, 0);
    let mut expired = ctx.clone();
    expired.now_ms = invite.expires_ms.clone();
    assert_eq!(
        closed_publish(&wire, &expired).unwrap_err().reason,
        "invite expired or already used"
    );
    let mut exhausted = context(&next, Some(accepted.projection));
    exhausted.invite = accepted.invite_update;
    exhausted.target = Some(DirectoryTarget {
        identity: next.identity.clone(),
        registered: true,
    });
    let next_wire = signed(
        &next,
        "second-join",
        None,
        EventBody::MemberAdded {
            identity: next.identity.clone(),
            role: Role::Member,
        },
        Some("ABCDEFGHJK"),
    );
    assert_eq!(
        closed_publish(&next_wire, &exhausted).unwrap_err().reason,
        "invite expired or already used"
    );
    let mut wrong = ctx;
    wrong.invite.as_mut().unwrap().code_hash = "0".repeat(64);
    assert!(!closed_publish(&wire, &wrong).unwrap_err().permanent);
}

#[test]
fn encrypted_genesis_and_encryption_transition_install_native_epoch_and_member_holds() {
    let owner = author(7);
    let wire = signed(&owner, "e2e", None, create(Privacy::EndToEnd), None);
    let state = closed_publish(&wire, &context(&owner, None))
        .unwrap()
        .projection;
    assert_eq!(state.privacy, Some(Privacy::EndToEnd));
    assert_eq!(state.mls.as_ref().unwrap().next_epoch, "0");
    assert_eq!(state.holds.len(), 1);
    assert_eq!(state.holds[0].identity, owner.identity);
    assert_eq!(state.holds[0].device, "*");
    assert_eq!(state.holds[0].seq, "0");
    let state = initial(&owner);
    let before = serde_json::to_value(&state).unwrap();
    assert!(before.get("mls").is_none());
    assert!(before.get("holds").is_none());
    assert!(before.get("pruned").is_none());
    let wire = signed(
        &owner,
        "encrypt",
        Some(&state),
        EventBody::SpaceEncrypted,
        None,
    );
    let ctx = context(&owner, Some(state));
    let accepted = closed_publish(&wire, &ctx).unwrap();
    assert_eq!(accepted.projection.privacy, Some(Privacy::EndToEnd));
    assert_eq!(accepted.projection.holds[0].seq, "1");
    assert_eq!(
        serde_json::to_value(ctx.projection.unwrap()).unwrap(),
        before
    );
}

#[test]
fn direct_chats_keep_two_person_limit_and_cannot_mint_invites() {
    let owner = author(7);
    let peer = author(17);
    let third = author(27);
    let create = EventBody::SpaceCreated {
        title: "Direct".into(),
        kind: SpaceKind::Direct,
        privacy: Privacy::Closed,
    };
    let wire = signed(&owner, "direct", None, create, None);
    let state = closed_publish(&wire, &context(&owner, None))
        .unwrap()
        .projection;
    let state = add(&owner, state, &peer, Role::Member);
    let wire = signed(
        &owner,
        "third",
        Some(&state),
        EventBody::MemberAdded {
            identity: third.identity.clone(),
            role: Role::Member,
        },
        None,
    );
    let mut ctx = context(&owner, Some(state.clone()));
    ctx.target = Some(DirectoryTarget {
        identity: third.identity,
        registered: true,
    });
    assert_eq!(
        closed_publish(&wire, &ctx).unwrap_err().reason,
        "a direct chat has two people"
    );
    let error=closed_invite_json(&serde_json::to_string(&state).unwrap(),&serde_json::json!({"who":owner.identity,"code":"ABCDEFGHJK","role":"Member","max_uses":1,"ttl_secs":"60","now_ms":"1000","existing":null}).to_string()).unwrap_err();
    assert_eq!(error.reason, "direct chats don't take invites");
}

#[test]
fn readers_and_unregistered_targets_keep_native_reasons_without_state_changes() {
    let owner = author(7);
    let reader = author(17);
    let unknown = author(27);
    let state = add(&owner, initial(&owner), &reader, Role::Reader);
    let wire = signed(&reader, "reader", Some(&state), message("denied"), None);
    assert_eq!(
        closed_publish(&wire, &context(&reader, Some(state.clone())))
            .unwrap_err()
            .reason,
        "readers can't write here"
    );
    let wire = signed(
        &owner,
        "unknown",
        Some(&state),
        EventBody::MemberAdded {
            identity: unknown.identity.clone(),
            role: Role::Member,
        },
        None,
    );
    let mut ctx = context(&owner, Some(state));
    ctx.target = Some(DirectoryTarget {
        identity: unknown.identity.clone(),
        registered: false,
    });
    assert_eq!(
        closed_publish(&wire, &ctx).unwrap_err().reason,
        "that person isn't on Zoen yet"
    );
    ctx.target.as_mut().unwrap().identity = owner.identity;
    let wrong_lookup = closed_publish(&wire, &ctx).unwrap_err();
    assert_eq!(wrong_lookup.code, "invalid_projection");
    assert!(!wrong_lookup.permanent);
}

#[test]
fn lookup_keys_are_derived_from_verified_signed_content() {
    let owner = author(7);
    let target = author(17);
    let state = initial(&owner);
    let wire = signed(
        &owner,
        "lookup",
        Some(&state),
        EventBody::MemberAdded {
            identity: target.identity.clone(),
            role: Role::Member,
        },
        Some("abcdefghjk"),
    );
    let keys: serde_json::Value = serde_json::from_str(
        &verify_publish_json(&wire, &serde_json::to_string(&session(&owner)).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(keys["lookup"]["target_identity"], target.identity);
    assert_eq!(keys["lookup"]["seen_seq"], "0");
    assert_eq!(
        keys["lookup"]["invite_hash"],
        relay_admission::code_hash("ABCDEFGHJK")
    );
}

#[test]
fn invite_policy_bounds_collision_and_admin_permissions_match_native_contract() {
    assert!(authorize_inviter(Some(Role::Admin), Role::Member).is_ok());
    assert_eq!(
        authorize_inviter(Some(Role::Admin), Role::Admin).unwrap_err(),
        "only the owner invites admins"
    );
    assert_eq!(
        authorize_inviter(Some(Role::Member), Role::Reader).unwrap_err(),
        "only owners and admins create invites"
    );
    assert_eq!(
        authorize_invite_kind(SpaceKind::Direct).unwrap_err(),
        "direct chats don't take invites"
    );
    assert_eq!(invite_limits(0, 0), (1, 60_000));
    assert_eq!(invite_limits(u32::MAX, u64::MAX), (10_000, 2_592_000_000));
    let owner = author(7);
    let state = initial(&owner);
    let invite = mint(&owner, &state, "ABCDEFGHJK", Role::Member, 1);
    let input = serde_json::json!({"who":owner.identity,"code":"abcdefghjk","role":Role::Member,"max_uses":1,"ttl_secs":"600","now_ms":"2000","existing":invite});
    let stored: ClosedInvite = serde_json::from_str(
        &closed_invite_json(&serde_json::to_string(&state).unwrap(), &input.to_string()).unwrap(),
    )
    .unwrap();
    assert_eq!(stored.expires_ms, "61000");
    let mut collision = input;
    collision["max_uses"] = serde_json::json!(2);
    assert_eq!(
        closed_invite_json(
            &serde_json::to_string(&state).unwrap(),
            &collision.to_string()
        )
        .unwrap_err()
        .reason,
        "invite code collision"
    );
}

#[test]
fn mismatched_projection_and_receipt_are_retryable_internal_failures() {
    let owner = author(7);
    let mut state = initial(&owner);
    state.space = "other".into();
    let wire = signed(&owner, "test", None, message("scope"), None);
    assert!(
        !closed_publish(&wire, &context(&owner, Some(state)))
            .unwrap_err()
            .permanent
    );
    let state = initial(&owner);
    assert!(
        !closed_replay_json(
            &wire,
            &serde_json::to_string(&session(&owner)).unwrap(),
            &serde_json::to_string(&state).unwrap(),
            &[0xff]
        )
        .unwrap_err()
        .permanent
    );
    assert!(!closed_publish_json(&wire, "{}").unwrap_err().permanent);
}
