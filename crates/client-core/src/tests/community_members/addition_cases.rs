//! Real account, signed membership, persisted outbox and JSON dispatcher.
use super::*;

pub(super) fn candidate(f: &Fixture, agent: bool) -> TestResult<String> {
    let mut e = f.client.lock();
    let mut p = e.identities.get(&f.member).ok_or("missing person")?.clone();
    p.id.clear();
    p.name = "New participant".into();
    p.handle = "new_participant".into();
    if agent {
        p.kind = types::IdentityKind::Agent;
    }
    Ok(e.create_identity(p, true)?)
}
pub(super) fn read(f: &Fixture, id: &str) -> Result<SpaceMemberContext, CoreError> {
    f.client
        .lock()
        .space_member_context(&f.space, id, &f.identity, &f.device)
}
fn add(f: &Fixture, id: &str, revision: &str) -> Result<SpaceMemberContext, CoreError> {
    f.client
        .lock()
        .add_member_reviewed(&f.space, id, &f.identity, &f.device, revision)
}
fn pending(f: &Fixture, id: &str) -> TestResult<types::Event> {
    f.client
        .lock()
        .store
        .outbox()?
        .into_iter()
        .find(|r| {
            !r.failed
                && r.event.space == f.space
                && matches!(&r.event.body,
            EventBody::MemberAdded { identity, role: Role::Member } if identity == id)
        })
        .map(|r| r.event)
        .ok_or_else(|| "missing MemberAdded".into())
}

#[test]
fn addition_is_only_member_once_and_pending_is_not_joined() -> TestResult<()> {
    for agent in [false, true] {
        let f = Fixture::new(SpaceKind::Group)?;
        let id = candidate(&f, agent)?;
        let review = read(&f, &id)?;
        assert!(review.can_add);
        assert!(review.member.is_none());
        let next = add(&f, &id, &review.revision)?;
        assert!(!next.can_add);
        assert_eq!(next.state, MemberActionState::Updating);
        assert!(
            next.member.is_some(),
            "optimistic profile is not a join receipt"
        );
        let event = pending(&f, &id)?;
        event_log::verify_author(&event)?;
        let counts = f.counts()?;
        assert!(add(&f, &id, &review.revision).is_err());
        assert_eq!(f.counts()?, counts);
        f.confirm(event)?;
        let joined = read(&f, &id)?;
        assert_eq!(joined.state, MemberActionState::Active);
        assert_eq!(joined.role.as_deref(), Some("member"));
        assert!(joined.delivery_ready);
        assert!(!joined.can_add);
        assert!(add(&f, &id, &review.revision).is_err());
        let e = f.client.lock();
        let log = e.logs.get(&f.space).ok_or("missing log")?;
        assert_eq!(
            log.events()
                .iter()
                .filter(|ev| matches!(&ev.body,
            EventBody::MemberAdded { identity, .. } if identity == &id))
                .count(),
            1
        );
        assert!(!log
            .events()
            .iter()
            .any(|ev| matches!(&ev.body, EventBody::GrantIssued { .. })));
    }
    Ok(())
}

#[test]
fn stale_actor_target_and_device_never_enqueue() -> TestResult<()> {
    let f = Fixture::new(SpaceKind::Community)?;
    let id = candidate(&f, false)?;
    let before = read(&f, &id)?;
    for role in [Role::Member, Role::Reader, Role::Admin] {
        f.ordered(EventBody::MemberAdded {
            identity: f.identity.clone(),
            role,
        })?;
        let counts = f.counts()?;
        assert!(add(&f, &id, &before.revision).is_err());
        assert_eq!(f.counts()?, counts);
    }
    let current = read(&f, &id)?;
    let counts = f.counts()?;
    assert!(f
        .client
        .lock()
        .add_member_reviewed(&f.space, &id, "foreign", &f.device, &current.revision)
        .is_err());
    assert!(f
        .client
        .lock()
        .add_member_reviewed(&f.space, &id, &f.identity, "foreign", &current.revision)
        .is_err());
    assert!(!read(&f, &f.member)?.can_add);
    assert!(!read(&f, &f.identity)?.can_add);
    assert!(!read(&f, "unknown")?.can_add);
    assert_eq!(f.counts()?, counts);
    f.ordered(EventBody::MemberAdded {
        identity: id.clone(),
        role: Role::Member,
    })?;
    f.ordered(EventBody::MemberRemoved {
        identity: id.clone(),
    })?;
    assert!(read(&f, &id)?.can_add);
    let counts = f.counts()?;
    assert!(add(&f, &id, &current.revision).is_err());
    assert_eq!(f.counts()?, counts);
    Ok(())
}

#[test]
fn rejected_addition_remains_absent_and_requires_explicit_fresh_review() -> TestResult<()> {
    let f = Fixture::new(SpaceKind::Group)?;
    let id = candidate(&f, true)?;
    let first = read(&f, &id)?;
    add(&f, &id, &first.revision)?;
    let event = pending(&f, &id)?;
    assert_eq!(
        f.client
            .lock()
            .reject(&event.client_id, "fixture denied", true, None),
        crate::sync::RejectionOutcome::UserFailed
    );
    let refused = read(&f, &id)?;
    assert!(refused.addition_failed);
    assert!(refused.can_add);
    assert_eq!(refused.state, MemberActionState::Removed);
    assert!(refused.member.is_none());
    let counts = f.counts()?;
    assert!(add(&f, &id, &first.revision).is_err());
    assert_eq!(f.counts()?, counts);
    Ok(())
}

#[tokio::test]
async fn json_dispatch_and_cold_reopen_preserve_the_exact_single_intent() -> TestResult<()> {
    let dir = std::env::temp_dir().join(types::new_id("native-add-member")?);
    std::fs::create_dir(&dir)?;
    let f = Fixture::at(
        dir.join("core.sqlite").to_string_lossy().into_owned(),
        SpaceKind::Group,
    )?;
    let id = candidate(&f, false)?;
    let list = serde_json::json!({ "op":"find_member_candidates", "query":"", "space_id":f.space,
        "expected_identity_id":f.identity, "expected_device_id":f.device });
    let found: Vec<SpaceMemberContext> =
        serde_json::from_str(&f.client.client_request(list.to_string()).await?)?;
    let review = found
        .iter()
        .find(|c| c.member_id == id)
        .ok_or("missing candidate")?;
    let command = serde_json::json!({ "op":"add_member_reviewed", "space_id":f.space, "identity_id":id,
        "expected_identity_id":f.identity, "expected_device_id":f.device, "observed_revision":review.revision });
    let result: SpaceMemberContext =
        serde_json::from_str(&f.client.client_request(command.to_string()).await?)?;
    assert_eq!(result.state, MemberActionState::Updating);
    let intent = pending(&f, &id)?;
    let f = f.cold_reopen()?;
    assert_eq!(pending(&f, &id)?.content, intent.content);
    let counts = f.counts()?;
    assert!(f.client.client_request(command.to_string()).await.is_err());
    assert_eq!(f.counts()?, counts);
    f.confirm(intent)?;
    assert_eq!(read(&f, &id)?.state, MemberActionState::Active);
    drop(f);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[tokio::test]
async fn lookup_rechecks_real_owner_and_role_before_installing_any_profile() -> TestResult<()> {
    for owner_change in [false, true] {
        let f = Fixture::new(SpaceKind::Group)?;
        let mut profile = f
            .client
            .lock()
            .identities
            .get(&f.member)
            .ok_or("missing profile")?
            .clone();
        profile.id = "e".repeat(64);
        profile.handle = "late_profile".into();
        let id = profile.id.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let lookup =
            f.client
                .member_candidates_lookup(&f.space, "late", &f.identity, &f.device, async {
                    rx.await.map_err(|_| CoreError::Storage {
                        message: "closed fixture".into(),
                    })
                });
        tokio::pin!(lookup);
        tokio::select! { result = &mut lookup => { result?; return Err("lookup completed before response".into()); }, () = tokio::task::yield_now() => {} }
        if owner_change {
            f.client.lock().me = Some(f.founder.clone());
        } else {
            f.ordered(EventBody::MemberAdded {
                identity: f.identity.clone(),
                role: Role::Member,
            })?;
        }
        tx.send(vec![profile])
            .map_err(|_| "missing lookup waiter")?;
        assert!(lookup.await.is_err());
        let e = f.client.lock();
        assert!(!e.identities.contains_key(&id));
        assert!(!e.store.identities()?.iter().any(|(p, _)| p.id == id));
    }
    Ok(())
}
