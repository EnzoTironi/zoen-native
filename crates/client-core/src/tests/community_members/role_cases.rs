use super::*;

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
