use super::*;

#[test]
fn role_assignment_cannot_replace_existing_owner() -> Result<(), Box<dyn std::error::Error>> {
    let author = Author::device(&Signer::generate()?, Signer::generate()?);
    let owner = Signer::generate()?.id();
    for actor_role in [Role::Owner, Role::Admin] {
        for role in [Role::Admin, Role::Member, Role::Reader] {
            let event = env(
                &author,
                Some(Seen {
                    seq: 3,
                    hash: "h3".into(),
                }),
                EventBody::MemberAdded {
                    identity: owner.clone(),
                    role,
                },
            )?;
            assert!(
                event.verify().is_ok(),
                "the authority negative has a genuine certified signature"
            );
            let facts = Facts {
                target_role: Some(Role::Owner),
                ..member_facts(actor_role)
            };
            assert!(
                admit(&event, &facts).is_err(),
                "a valid signature cannot demote the owner"
            );
        }
    }
    Ok(())
}
