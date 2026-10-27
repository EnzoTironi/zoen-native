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

#[test]
fn signed_membership_review_must_observe_target_and_administrator_removal_cuts(
) -> Result<(), Box<dyn std::error::Error>> {
    let author = Author::device(&Signer::generate()?, Signer::generate()?);
    let target = Signer::generate()?.id();
    for current in [None, Some(Role::Reader)] {
        for (target_cut, author_cut) in [(Some(5), None), (None, Some(5)), (Some(5), Some(6))] {
            let stale = env(
                &author,
                Some(Seen {
                    seq: 4,
                    hash: "h4".into(),
                }),
                EventBody::MemberAdded {
                    identity: target.clone(),
                    role: Role::Admin,
                },
            )?;
            stale.verify().map_err(|error| format!("{error:?}"))?;
            let facts = Facts {
                head: Some((9, "h9".into())),
                seen_hash: Some("h4".into()),
                target_role: current,
                target_removed_at: target_cut,
                author_removed_at: author_cut,
                ..member_facts(Role::Owner)
            };
            assert!(
                admit(&stale, &facts).is_err(),
                "an authentic old signature cannot regrant after a membership cut"
            );
            let fresh = env(
                &author,
                Some(Seen {
                    seq: 6,
                    hash: "h6".into(),
                }),
                EventBody::MemberAdded {
                    identity: target.clone(),
                    role: Role::Admin,
                },
            )?;
            let facts = Facts {
                seen_hash: Some("h6".into()),
                ..facts
            };
            assert_eq!(
                admit(&fresh, &facts).map_err(|error| format!("{error:?}"))?,
                Effect::Add {
                    identity: target.clone(),
                    role: Role::Admin,
                    by_invite: false
                },
                "a new explicit review may observe the cut; unrelated later messages are harmless"
            );
        }
    }
    Ok(())
}
