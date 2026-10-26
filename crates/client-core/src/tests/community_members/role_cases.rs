use super::*;

fn choice(context: &SpaceMemberContext, role: &str) -> TestResult<String> {
    context
        .role_options
        .iter()
        .find(|option| option.role == role)
        .map(|option| option.revision.clone())
        .ok_or_else(|| "role option missing".into())
}
fn assign(f: &Fixture, role: &str, revision: &str) -> Result<SpaceMemberContext, CoreError> {
    f.client.set_member_role_reviewed(
        f.space.clone(),
        f.member.clone(),
        f.identity.clone(),
        f.device.clone(),
        revision.into(),
        role.into(),
    )
}
fn pending(f: &Fixture) -> TestResult<types::Event> {
    f.client.lock().store.outbox()?.into_iter().find(|row| !row.failed
        && row.event.space == f.space
        && matches!(&row.event.body, EventBody::MemberAdded { identity, .. } if identity == &f.member))
        .map(|row| row.event).ok_or_else(|| "role intent missing".into())
}

#[test]
fn role_assignment_cannot_replace_existing_owner() -> TestResult<()> {
    let f = Fixture::new(SpaceKind::Group)?;
    let before = f.counts()?;
    assert!(
        f.client
            .add_admin(f.space.clone(), f.founder.clone())
            .is_err(),
        "an administrator must not replace the canonical owner's role"
    );
    assert_eq!(
        f.counts()?,
        before,
        "no optimistic role or signed intent is admitted"
    );
    assert_eq!(
        f.client
            .member_roles(f.space.clone())?
            .into_iter()
            .find(|role| role.identity_id == f.founder)
            .ok_or("owner missing")?
            .role,
        "owner"
    );
    Ok(())
}

#[test]
fn role_review_keeps_canonical_state_and_survives_unknown_response_and_cold() -> TestResult<()> {
    for kind in [SpaceKind::Group, SpaceKind::Community] {
        let directory = std::env::temp_dir().join(types::new_id("member_role")?);
        std::fs::create_dir_all(&directory)?;
        let mut f = Fixture::at(
            directory.join("core.sqlite").to_string_lossy().into_owned(),
            kind,
        )?;
        for (before, desired) in [
            ("member", "reader"),
            ("reader", "admin"),
            ("admin", "member"),
        ] {
            let review = f.context()?;
            assert_eq!(review.role.as_deref(), Some(before));
            let revision = choice(&review, desired)?;
            assert_ne!(
                revision, review.revision,
                "role itself is bound by the option review"
            );
            let next = assign(&f, desired, &revision)?;
            assert_eq!(next.state, MemberActionState::Updating);
            assert_eq!(next.role.as_deref(), Some(before));
            assert_eq!(next.pending_role.as_deref(), Some(desired));
            assert!(next.role_options.is_empty());
            assert_eq!(
                f.client
                    .member_roles(f.space.clone())?
                    .into_iter()
                    .find(|member| member.identity_id == f.member)
                    .ok_or("member missing")?
                    .role,
                before
            );
            let event = pending(&f)?;
            event_log::verify_author(&event)?;
            let counts = f.counts()?;
            assert!(assign(&f, desired, &revision).is_err());
            assert_eq!(f.counts()?, counts);
            f = f.cold_reopen()?;
            assert_eq!(f.context()?.pending_role.as_deref(), Some(desired));
            assert!(assign(&f, desired, &revision).is_err());
            f.confirm(event)?;
            let confirmed = f.context()?;
            assert_eq!(confirmed.state, MemberActionState::Active);
            assert_eq!(confirmed.role.as_deref(), Some(desired));
            assert!(confirmed.pending_role.is_none());
            assert!(!confirmed
                .role_options
                .iter()
                .any(|option| option.role == desired));
            f = f.cold_reopen()?;
            assert_eq!(f.context()?, confirmed);
            let counts = f.counts()?;
            assert!(
                assign(&f, desired, &revision).is_err(),
                "an old response cannot sign again"
            );
            assert_eq!(f.counts()?, counts);
        }
        drop(f);
        std::fs::remove_dir_all(directory)?;
    }
    Ok(())
}

#[test]
fn role_review_binds_desired_role_current_actor_target_device_and_membership_aba() -> TestResult<()>
{
    let f = Fixture::new(SpaceKind::Community)?;
    let review = f.context()?;
    let reader = choice(&review, "reader")?;
    let before = f.counts()?;
    assert!(assign(&f, "admin", &reader).is_err());
    assert!(assign(&f, "owner", &reader).is_err());
    assert!(assign(&f, "reader", &review.revision).is_err());
    for (target, actor, device) in [
        (f.founder.as_str(), f.identity.as_str(), f.device.as_str()),
        (f.identity.as_str(), f.identity.as_str(), f.device.as_str()),
        (f.member.as_str(), "foreign", f.device.as_str()),
        (f.member.as_str(), f.identity.as_str(), "foreign"),
    ] {
        assert!(f
            .client
            .set_member_role_reviewed(
                f.space.clone(),
                target.into(),
                actor.into(),
                device.into(),
                reader.clone(),
                "reader".into()
            )
            .is_err());
    }
    assert_eq!(f.counts()?, before);
    f.ordered(EventBody::MessagePosted {
        message: types::new_id("message")?,
        text: "unrelated conversation".into(),
        attaches: None,
        media: None,
        reply: None,
        forwarded: false,
    })?;
    assert_eq!(choice(&f.context()?, "reader")?, reader);
    for who in [&f.member, &f.identity] {
        let old = choice(&f.context()?, "reader")?;
        let restored = if who == &f.member {
            Role::Member
        } else {
            Role::Admin
        };
        for role in [Role::Reader, restored] {
            f.ordered(EventBody::MemberAdded {
                identity: who.clone(),
                role,
            })?;
        }
        assert_ne!(choice(&f.context()?, "reader")?, old);
        let counts = f.counts()?;
        assert!(assign(&f, "reader", &old).is_err());
        assert_eq!(f.counts()?, counts);
    }
    let current = choice(&f.context()?, "reader")?;
    f.client
        .lock()
        .store
        .set_meta("devices.revoked", &serde_json::to_string(&vec![&f.device])?)?;
    let counts = f.counts()?;
    assert!(assign(&f, "reader", &current).is_err());
    assert_eq!(f.counts()?, counts);
    Ok(())
}

#[test]
fn role_refusal_is_durable_only_for_the_exact_still_current_membership_cut() -> TestResult<()> {
    let directory = std::env::temp_dir().join(types::new_id("role_refusal")?);
    std::fs::create_dir_all(&directory)?;
    let f = Fixture::at(
        directory.join("core.sqlite").to_string_lossy().into_owned(),
        SpaceKind::Group,
    )?;
    let first = choice(&f.context()?, "reader")?;
    assign(&f, "reader", &first)?;
    let event = pending(&f)?;
    assert_eq!(
        f.client
            .lock()
            .reject(&event.client_id, "synthetic role refusal", true, None),
        crate::sync::RejectionOutcome::UserFailed
    );
    let refused = f.context()?;
    assert!(refused.role_change_failed);
    assert!(!refused.removal_failed && !refused.addition_failed);
    assert_eq!(refused.role.as_deref(), Some("member"));
    let f = f.cold_reopen()?;
    assert_eq!(f.context()?, refused);
    assert!(assign(&f, "reader", &first).is_err());
    let counts = f.counts()?;
    for _ in 0..2 {
        assert_eq!(f.context()?, refused);
    }
    assert_eq!(f.counts()?, counts, "reads never retry failed role writes");
    for role in [Role::Reader, Role::Member] {
        f.ordered(EventBody::MemberAdded {
            identity: f.member.clone(),
            role,
        })?;
    }
    let current = f.context()?;
    assert!(
        !current.role_change_failed,
        "ABA retires the exact old refusal"
    );
    assign(&f, "reader", &choice(&current, "reader")?)?;
    f.confirm(pending(&f)?)?;
    assert_eq!(f.context()?.role.as_deref(), Some("reader"));
    drop(f);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[tokio::test]
async fn role_json_contract_uses_the_same_scoped_choice_and_core_lock() -> TestResult<()> {
    let f = Fixture::new(SpaceKind::Group)?;
    let query = serde_json::json!({ "op":"space_member_context", "space_id":f.space,
        "identity_id":f.member,"expected_identity_id":f.identity,"expected_device_id":f.device });
    let before: SpaceMemberContext =
        serde_json::from_str(&f.client.client_request(query.to_string()).await?)?;
    let command = serde_json::json!({ "op":"set_member_role_reviewed", "space_id":f.space,
        "identity_id":f.member,"expected_identity_id":f.identity,"expected_device_id":f.device,
        "observed_revision":choice(&before,"reader")?,"role":"reader" });
    let next: SpaceMemberContext =
        serde_json::from_str(&f.client.client_request(command.to_string()).await?)?;
    assert_eq!(next.state, MemberActionState::Updating);
    assert_eq!(next.role.as_deref(), Some("member"));
    assert_eq!(next.pending_role.as_deref(), Some("reader"));
    let counts = f.counts()?;
    assert!(f.client.client_request(command.to_string()).await.is_err());
    assert_eq!(f.counts()?, counts);
    f.confirm(pending(&f)?)?;
    let after: SpaceMemberContext =
        serde_json::from_str(&f.client.client_request(query.to_string()).await?)?;
    assert_eq!(after.role.as_deref(), Some("reader"));
    assert!(after.pending_role.is_none());
    Ok(())
}
