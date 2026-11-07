//! Invitation limits reuse the exact native self-membership review, never UI role labels.
use super::*;
use crate::client_api::space_invite_command::{check, SpaceInviteInput};

fn input(f: &Fixture) -> TestResult<SpaceInviteInput> {
    let context = f.client.space_member_context(
        f.space.clone(),
        f.identity.clone(),
        f.identity.clone(),
        f.device.clone(),
    )?;
    Ok(SpaceInviteInput {
        space_id: f.space.clone(),
        expected_identity_id: f.identity.clone(),
        expected_device_id: f.device.clone(),
        observed_revision: context.revision,
        max_uses: 50,
        ttl_secs: 604_800,
    })
}
#[test]
fn invite_review_accepts_native_admin_limits_without_creating_local_grants() -> TestResult<()> {
    for kind in [SpaceKind::Group, SpaceKind::Community] {
        let f = Fixture::new(kind)?;
        let before = f.counts()?;
        for (uses, ttl) in [(1, 60), (50, 604_800), (10_000, 2_592_000)] {
            let mut selected = input(&f)?;
            selected.max_uses = uses;
            selected.ttl_secs = ttl;
            check(&f.client, &selected)?;
        }
        assert_eq!(f.counts()?, before);
    }
    Ok(())
}
#[test]
fn invite_review_refuses_invalid_limits_owner_device_and_membership_aba() -> TestResult<()> {
    let f = Fixture::new(SpaceKind::Group)?;
    let original = input(&f)?;
    for (uses, ttl) in [(0, 3600), (10_001, 3600), (1, 59), (1, 2_592_001)] {
        let mut invalid = original.clone();
        invalid.max_uses = uses;
        invalid.ttl_secs = ttl;
        assert!(check(&f.client, &invalid).is_err());
    }
    let mut invalid = original.clone();
    invalid.expected_identity_id = f.founder.clone();
    assert!(check(&f.client, &invalid).is_err());
    invalid = original.clone();
    invalid.expected_device_id = "another-device".into();
    assert!(check(&f.client, &invalid).is_err());
    for role in [Role::Member, Role::Admin] {
        f.ordered(EventBody::MemberAdded {
            identity: f.identity.clone(),
            role,
        })?;
        assert!(
            check(&f.client, &original).is_err(),
            "old review cannot survive a role cut"
        );
    }
    check(&f.client, &input(&f)?)?;
    Ok(())
}
#[test]
fn invite_review_refuses_direct_chat_even_with_owner_looking_labels() -> TestResult<()> {
    let f = Fixture::new(SpaceKind::Direct)?;
    let selected = SpaceInviteInput {
        space_id: f.space.clone(),
        expected_identity_id: f.identity.clone(),
        expected_device_id: f.device.clone(),
        observed_revision: "ab".repeat(32),
        max_uses: 1,
        ttl_secs: 3600,
    };
    assert!(check(&f.client, &selected).is_err());
    Ok(())
}

#[test]
fn invitation_management_uses_the_same_review_without_limit_placeholders() -> TestResult<()> {
    use crate::client_api::space_invite_command::{check_review, SpaceInviteReview};
    let f = Fixture::new(SpaceKind::Group)?;
    let original = input(&f)?;
    let review = SpaceInviteReview {
        space_id: original.space_id,
        expected_identity_id: original.expected_identity_id,
        expected_device_id: original.expected_device_id,
        observed_revision: original.observed_revision,
    };
    check_review(&f.client, &review)?;
    f.ordered(EventBody::MemberAdded {
        identity: f.identity.clone(),
        role: Role::Member,
    })?;
    assert!(check_review(&f.client, &review).is_err());
    Ok(())
}
