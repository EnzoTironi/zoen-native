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

#[test]
fn role_assignment_reuses_authority_at_member_limit_and_binds_the_signed_role(
) -> Result<(), Box<dyn std::error::Error>> {
    let author = Author::device(&Signer::generate()?, Signer::generate()?);
    let target = Signer::generate()?.id();
    for current in [Role::Member, Role::Reader, Role::Admin] {
        for desired in [Role::Member, Role::Reader, Role::Admin] {
            let event = env(
                &author,
                Some(Seen {
                    seq: 3,
                    hash: "h3".into(),
                }),
                EventBody::MemberAdded {
                    identity: target.clone(),
                    role: desired,
                },
            )?;
            for actor in [Role::Owner, Role::Admin] {
                let facts = Facts {
                    target_role: Some(current),
                    member_count: MAX_MEMBERS,
                    ..member_facts(actor)
                };
                assert_eq!(
                    admit(&event, &facts).map_err(|reject| format!("{reject:?}"))?,
                    Effect::Add {
                        identity: target.clone(),
                        role: desired,
                        by_invite: false
                    }
                );
            }
            for actor in [Role::Member, Role::Reader] {
                let facts = Facts {
                    target_role: Some(current),
                    ..member_facts(actor)
                };
                assert!(admit(&event, &facts).is_err());
            }
        }
    }
    let mut changed = author.sign_event(
        "sp_1",
        "role-binding",
        1,
        Some(Seen {
            seq: 3,
            hash: "h3".into(),
        }),
        EventBody::MemberAdded {
            identity: target.clone(),
            role: Role::Reader,
        },
    )?;
    event_log::verify_author(&changed)?;
    changed.body = EventBody::MemberAdded {
        identity: target,
        role: Role::Admin,
    };
    assert!(
        event_log::verify_author(&changed).is_err(),
        "desired role cannot be substituted behind a valid certificate"
    );
    Ok(())
}
