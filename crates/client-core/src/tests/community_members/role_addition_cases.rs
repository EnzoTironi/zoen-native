//! Initial access is one reviewed MemberAdded, never a temporary writable member.
use super::addition_cases::{candidate, read};
use super::*;

fn choice(context: &SpaceMemberContext, desired: &str) -> TestResult<String> {
    context
        .role_options
        .iter()
        .find(|choice| choice.role == desired)
        .map(|choice| choice.revision.clone())
        .ok_or_else(|| "initial role option missing".into())
}
fn assign(
    f: &Fixture,
    id: &str,
    revision: &str,
    role: &str,
) -> Result<SpaceMemberContext, CoreError> {
    f.client.set_member_role_reviewed(
        f.space.clone(),
        id.into(),
        f.identity.clone(),
        f.device.clone(),
        revision.into(),
        role.into(),
    )
}
fn intent(f: &Fixture, id: &str) -> TestResult<types::Event> {
    f.client
        .lock()
        .store
        .outbox()?
        .into_iter()
        .find(|row| {
            !row.failed
                && row.event.space == f.space
                && matches!(&row.event.body,
            EventBody::MemberAdded { identity, .. } if identity == id)
        })
        .map(|row| row.event)
        .ok_or_else(|| "initial role intent missing".into())
}

#[test]
fn first_addition_has_exact_reviewed_role_before_and_after_cold_receipt() -> TestResult<()> {
    for (kind, desired, role) in [
        (SpaceKind::Group, "reader", Role::Reader),
        (SpaceKind::Community, "admin", Role::Admin),
    ] {
        let dir = std::env::temp_dir().join(types::new_id("initial-role")?);
        std::fs::create_dir(&dir)?;
        let f = Fixture::at(dir.join("core.sqlite").to_string_lossy().into_owned(), kind)?;
        let id = candidate(&f, false)?;
        let context = read(&f, &id)?;
        assert_eq!(context.state, MemberActionState::Removed);
        assert!(context.can_add && context.candidate.is_some());
        assert!(context.role.is_none() && context.member.is_none());
        assert_eq!(
            context
                .role_options
                .iter()
                .map(|choice| choice.role.as_str())
                .collect::<Vec<_>>(),
            ["member", "reader", "admin"]
        );
        let revision = choice(&context, desired)?;
        let counts = f.counts()?;
        assert!(assign(&f, &id, &revision, "owner").is_err());
        assert!(assign(&f, &id, &revision, "member").is_err());
        assert_eq!(f.counts()?, counts);
        let pending = assign(&f, &id, &revision, desired)?;
        assert_eq!(pending.state, MemberActionState::Updating);
        assert_eq!(pending.pending_role.as_deref(), Some(desired));
        assert!(pending.role.is_none() && pending.role_options.is_empty());
        assert!(!f
            .client
            .member_roles(f.space.clone())?
            .iter()
            .any(|m| m.identity_id == id));
        let event = intent(&f, &id)?;
        event_log::verify_author(&event)?;
        assert!(
            matches!(&event.body, EventBody::MemberAdded { identity, role: r } if identity == &id && *r == role)
        );
        let f = f.cold_reopen()?;
        assert_eq!(read(&f, &id)?.pending_role.as_deref(), Some(desired));
        assert_eq!(intent(&f, &id)?, event);
        let counts = f.counts()?;
        assert!(assign(&f, &id, &revision, desired).is_err());
        assert_eq!(f.counts()?, counts);
        f.confirm(event)?;
        let confirmed = read(&f, &id)?;
        assert_eq!(confirmed.state, MemberActionState::Active);
        assert_eq!(confirmed.role.as_deref(), Some(desired));
        assert!(confirmed.pending_role.is_none());
        let f = f.cold_reopen()?;
        assert_eq!(read(&f, &id)?, confirmed);
        {
            let engine = f.client.lock();
            let additions: Vec<_> = engine
                .logs
                .get(&f.space)
                .ok_or("log missing")?
                .events()
                .iter()
                .filter_map(|event| match &event.body {
                    EventBody::MemberAdded { identity, role } if identity == &id => Some(*role),
                    _ => None,
                })
                .collect();
            assert_eq!(
                additions,
                [role],
                "no intermediate writable Member event exists"
            );
        }
        drop(f);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn refused_initial_reader_stays_absent_and_old_choice_cannot_retry() -> TestResult<()> {
    let f = Fixture::new(SpaceKind::Group)?;
    let id = candidate(&f, false)?;
    let revision = choice(&read(&f, &id)?, "reader")?;
    assign(&f, &id, &revision, "reader")?;
    let event = intent(&f, &id)?;
    assert_eq!(
        f.client
            .lock()
            .reject(&event.client_id, "synthetic admission refusal", true, None),
        crate::sync::RejectionOutcome::UserFailed
    );
    let context = read(&f, &id)?;
    assert!(context.addition_failed && !context.role_change_failed);
    assert!(context.role.is_none() && context.member.is_none());
    assert_eq!(context.state, MemberActionState::Removed);
    let counts = f.counts()?;
    assert!(assign(&f, &id, &revision, "reader").is_err());
    assert_eq!(f.counts()?, counts);
    assert_ne!(choice(&context, "reader")?, revision);
    Ok(())
}
