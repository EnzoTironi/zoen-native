//! Signed native account, canonical membership and real persisted pending/refusal cuts.
use super::*;
type TestResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;
mod support;
use support::Fixture;
use types::{EventBody, Role, SpaceKind};

#[test]
fn pending_removal_is_not_confirmed_duplicate_or_cold_retry() -> TestResult<()> {
    for kind in [SpaceKind::Group, SpaceKind::Community] {
        let f = Fixture::new(kind)?;
        let review = f.context()?;
        assert!(review.can_remove);
        let next = f.remove(&review.revision)?;
        assert_eq!(next.state, MemberActionState::Removing);
        assert!(!next.can_remove);
        assert!(
            next.member.is_some(),
            "confirmed member still exists before relay ACK"
        );
        assert!(
            !f.client
                .space(f.space.clone())?
                .members
                .iter()
                .any(|p| p.id == f.member),
            "ordinary UI projection is optimistic"
        );
        let counts = f.counts()?;
        assert!(f.remove(&review.revision).is_err());
        assert_eq!(f.counts()?, counts);
        f.client.lock().reload()?;
        assert_eq!(f.context()?.state, MemberActionState::Removing);
        assert!(f.remove(&review.revision).is_err());
        assert_eq!(f.counts()?, counts);
        f.confirm(f.pending_removal()?)?;
        assert_eq!(f.context()?.state, MemberActionState::Removed);
        let counts = f.counts()?;
        assert!(f.remove(&review.revision).is_err());
        assert_eq!(
            f.counts()?,
            counts,
            "lost response never appends another removal"
        );
    }
    Ok(())
}

#[test]
fn ordinary_messages_keep_review_but_target_and_actor_role_or_rejoin_aba_do_not() -> TestResult<()>
{
    let f = Fixture::new(SpaceKind::Community)?;
    let first = f.context()?;
    f.ordered(EventBody::MessagePosted {
        message: types::new_id("message"),
        text: "Unrelated canonical message".into(),
        attaches: None,
        reply: None,
        forwarded: false,
        media: None,
    })?;
    assert_eq!(f.context()?.revision, first.revision);
    for (identity, states) in [
        (&f.member, vec![Some(Role::Reader), Some(Role::Member)]),
        (&f.member, vec![None, Some(Role::Member)]),
        (&f.identity, vec![Some(Role::Member), Some(Role::Admin)]),
        (&f.identity, vec![None, Some(Role::Admin)]),
    ] {
        let before = f.context()?;
        for role in states {
            f.ordered(match role {
                Some(role) => EventBody::MemberAdded {
                    identity: identity.clone(),
                    role,
                },
                None => EventBody::MemberRemoved {
                    identity: identity.clone(),
                },
            })?;
        }
        assert!(f.context()?.can_remove);
        assert_ne!(f.context()?.revision, before.revision);
        let counts = f.counts()?;
        assert!(f.remove(&before.revision).is_err());
        assert_eq!(f.counts()?, counts);
    }
    Ok(())
}

#[test]
fn current_permissions_device_revocation_direct_and_self_fail_before_writes() -> TestResult<()> {
    let f = Fixture::new(SpaceKind::Group)?;
    let review = f.context()?;
    for role in [Role::Reader, Role::Member] {
        f.ordered(EventBody::MemberAdded {
            identity: f.identity.clone(),
            role,
        })?;
        assert!(!f.context()?.can_remove);
        let counts = f.counts()?;
        assert!(f.remove(&review.revision).is_err());
        assert_eq!(f.counts()?, counts);
    }
    f.ordered(EventBody::MemberAdded {
        identity: f.identity.clone(),
        role: Role::Admin,
    })?;
    let mut e = f.client.lock();
    let counts = (e.store.event_count()?, e.store.outbox()?.len());
    let own = e.space_member_context(&f.space, &f.identity, &f.identity, &f.device)?;
    assert!(!own.can_remove);
    assert!(e
        .remove_member_reviewed(&f.space, &f.identity, &f.identity, &f.device, &own.revision)
        .is_err());
    assert!(e
        .space_member_context(&f.space, &f.member, "foreign", &f.device)
        .is_err());
    assert!(e
        .space_member_context(&f.space, &f.member, &f.identity, "foreign")
        .is_err());
    assert!(
        !e.space_member_context(&f.space, &f.founder, &f.identity, &f.device)?
            .can_remove,
        "admin cannot remove owner"
    );
    assert_eq!((e.store.event_count()?, e.store.outbox()?.len()), counts);
    e.store
        .set_meta("devices.revoked", &serde_json::to_string(&vec![&f.device])?)?;
    assert!(e
        .space_member_context(&f.space, &f.member, &f.identity, &f.device)
        .is_err());
    drop(e);
    let counts = f.counts()?;
    assert!(f.remove(&review.revision).is_err());
    assert_eq!(f.counts()?, counts);
    let direct = Fixture::new(SpaceKind::Direct)?;
    let counts = direct.counts()?;
    assert!(direct.context().is_err());
    assert!(direct.remove(&review.revision).is_err());
    assert_eq!(direct.counts()?, counts);
    Ok(())
}

#[test]
fn pending_role_change_never_admits_a_reviewed_removal() -> TestResult<()> {
    let f = Fixture::new(SpaceKind::Community)?;
    let review = f.context()?;
    f.client
        .lock()
        .add_member(&f.space, &f.member, Role::Reader)?;
    let current = f.context()?;
    assert_eq!(current.state, MemberActionState::Updating);
    assert!(!current.can_remove);
    assert_eq!(
        current.role.as_deref(),
        Some("member"),
        "role remains canonical"
    );
    let counts = f.counts()?;
    assert!(f.remove(&review.revision).is_err());
    assert!(f.remove(&current.revision).is_err());
    assert_eq!(f.counts()?, counts);
    let pending = f
        .client
        .lock()
        .store
        .outbox()?
        .into_iter()
        .find(|row| {
            row.event.space == f.space
                && !row.failed
                && matches!(&row.event.body,
            EventBody::MemberAdded { identity, role: Role::Reader } if identity == &f.member)
        })
        .ok_or("missing signed pending role change")?
        .event;
    f.confirm(pending)?;
    assert_eq!(f.context()?.state, MemberActionState::Active);
    assert_eq!(f.context()?.role.as_deref(), Some("reader"));
    assert_ne!(f.context()?.revision, review.revision);
    Ok(())
}

#[test]
fn permanent_refusal_survives_cold_reopen_and_confirmed_removal_clears_it() -> TestResult<()> {
    let directory = std::env::temp_dir().join(types::new_id("membership-refusal"));
    std::fs::create_dir(&directory)?;
    let f = Fixture::at(
        directory.join("core.sqlite").to_string_lossy().into_owned(),
        SpaceKind::Community,
    )?;
    let review = f.context()?;
    assert!(!review.removal_failed);
    assert!(!f.remove(&review.revision)?.removal_failed);
    let failed = f.reject_removal()?;
    let current = f.context()?;
    assert_eq!(current.state, MemberActionState::Active);
    assert!(current.member.is_some() && current.can_remove && current.removal_failed);
    assert_ne!(current.revision, review.revision);
    assert!(f
        .client
        .lock()
        .store
        .outbox()?
        .iter()
        .any(|p| p.event.client_id == failed.client_id && p.failed));
    let f = f.cold_reopen()?;
    assert_eq!(f.context()?, current);
    let counts = f.counts()?;
    assert!(f.remove(&review.revision).is_err());
    assert_eq!(f.counts()?, counts);
    // A new explicit review may retry; reads and reopen never enqueue it.
    assert!(!f.remove(&current.revision)?.removal_failed);
    f.confirm(f.pending_removal()?)?;
    assert_eq!(f.context()?.state, MemberActionState::Removed);
    assert!(!f.context()?.removal_failed);
    let f = f.cold_reopen()?;
    assert_eq!(f.context()?.state, MemberActionState::Removed);
    assert!(!f.context()?.removal_failed);
    drop(f);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn refusal_is_exact_writer_target_and_membership_cut_not_unrelated_or_old_history() -> TestResult<()>
{
    let f = Fixture::new(SpaceKind::Community)?;
    for sibling in [true, false] {
        f.retain_other_writer_refusal(sibling)?;
        assert!(!f.context()?.removal_failed);
    }
    for who in [&f.member, &f.identity] {
        f.remove(&f.context()?.revision)?;
        f.reject_removal()?;
        assert!(f.context()?.removal_failed);
        assert!(
            !f.client
                .lock()
                .space_member_context(&f.space, &f.founder, &f.identity, &f.device)?
                .removal_failed
        );
        f.ordered(EventBody::MessagePosted {
            message: types::new_id("message"),
            text: "ordinary message does not erase refusal".into(),
            attaches: None,
            reply: None,
            forwarded: false,
            media: None,
        })?;
        assert!(f.context()?.removal_failed);
        let role = if who == &f.identity {
            Role::Admin
        } else {
            Role::Member
        };
        f.ordered(EventBody::MemberRemoved {
            identity: who.clone(),
        })?;
        f.ordered(EventBody::MemberAdded {
            identity: who.clone(),
            role,
        })?;
        assert!(f.context()?.can_remove);
        assert!(
            !f.context()?.removal_failed,
            "rejoin retires old actor/target failure"
        );
    }
    Ok(())
}

#[test]
fn enqueue_failure_keeps_projection_and_post_enqueue_read_failure_retains_exact_intent(
) -> TestResult<()> {
    let f = Fixture::new(SpaceKind::Group)?;
    let review = f.context()?;
    let counts = f.counts()?;
    f.client.lock().store.conn().execute_batch("CREATE TEMP TRIGGER reject_member BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT,'synthetic membership write failure'); END;")?;
    assert!(f.remove(&review.revision).is_err());
    assert_eq!(f.counts()?, counts);
    assert_eq!(f.context()?, review);
    f.client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER reject_member")?;
    f.client.lock().store.set_meta("devices.revoked", "[]")?;
    f.client.lock().store.conn().execute_batch("CREATE TEMP TRIGGER lose_context AFTER INSERT ON outbox BEGIN UPDATE meta SET value='invalid-json' WHERE key='devices.revoked'; END;")?;
    assert!(matches!(
        f.remove(&review.revision),
        Err(CoreError::Storage { .. })
    ));
    let pending = f.pending_removal()?;
    assert!(f.context().is_err());
    let after = f.counts()?;
    assert!(f.remove(&review.revision).is_err());
    assert_eq!(f.counts()?, after);
    f.client
        .lock()
        .store
        .conn()
        .execute_batch("DROP TRIGGER lose_context")?;
    f.client.lock().store.set_meta("devices.revoked", "[]")?;
    assert_eq!(f.context()?.state, MemberActionState::Removing);
    assert_eq!(f.pending_removal()?.client_id, pending.client_id);
    Ok(())
}

#[tokio::test]
async fn actual_json_bridge_and_cold_reopen_keep_one_signed_pending_removal() -> TestResult<()> {
    let directory = std::env::temp_dir().join(types::new_id("community-member-test"));
    std::fs::create_dir(&directory)?;
    let f = Fixture::at(
        directory.join("core.sqlite").to_string_lossy().into_owned(),
        SpaceKind::Community,
    )?;
    let query = serde_json::json!({ "op":"space_member_context", "space_id":f.space,
        "identity_id":f.member, "expected_identity_id":f.identity, "expected_device_id":f.device });
    let review: SpaceMemberContext =
        serde_json::from_str(&f.client.client_request(query.to_string()).await?)?;
    let remove = serde_json::json!({ "op":"remove_member_reviewed", "space_id":f.space,
        "identity_id":f.member, "expected_identity_id":f.identity, "expected_device_id":f.device,
        "observed_revision":review.revision });
    let result: SpaceMemberContext =
        serde_json::from_str(&f.client.client_request(remove.to_string()).await?)?;
    assert_eq!(result.state, MemberActionState::Removing);
    let original = f.pending_removal()?;
    let f = f.cold_reopen()?;
    assert_eq!(f.context()?.state, MemberActionState::Removing);
    assert_eq!(f.pending_removal()?.content, original.content);
    let counts = f.counts()?;
    assert!(f.client.client_request(remove.to_string()).await.is_err());
    assert_eq!(f.counts()?, counts);
    f.confirm(original)?;
    let f = f.cold_reopen()?;
    let confirmed: SpaceMemberContext =
        serde_json::from_str(&f.client.client_request(query.to_string()).await?)?;
    assert_eq!(confirmed.state, MemberActionState::Removed);
    assert_eq!(confirmed.member, None);
    drop(f);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
