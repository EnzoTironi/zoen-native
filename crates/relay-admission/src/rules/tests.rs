use super::*;
use event_log::{Author, Signer};
use types::Seen;

fn env(
    author: &Author,
    seen: Option<Seen>,
    body: EventBody,
) -> Result<Envelope, Box<dyn std::error::Error>> {
    let event = author.sign_event("sp_1", "c", 1, seen, body)?;
    Ok(Envelope::plain(&event)?)
}

fn member_facts(role: Role) -> Facts {
    Facts {
        head: Some((3, "h3".into())),
        kind: Some(SpaceKind::Group),
        author_role: Some(role),
        member_count: 3,
        seen_hash: Some("h3".into()),
        target_known: true,
        ..Facts::default()
    }
}

fn e2e_facts(role: Role) -> Facts {
    Facts {
        privacy: Some(Privacy::EndToEnd),
        ..member_facts(role)
    }
}

fn sealed(author: &Author, kind: SealedKind) -> Result<Envelope, Box<dyn std::error::Error>> {
    let seen = Seen {
        seq: 3,
        hash: "h3".into(),
    };
    Ok(Envelope::sealed(
        author,
        "sp_1",
        "c",
        1,
        Some(&seen),
        protocol::Sealed::new(kind, 3, vec![1, 2, 3]),
    )?)
}

fn checkpoint(seq: u64, hash: &str) -> EventBody {
    EventBody::Checkpoint {
        upto: Seen {
            seq,
            hash: hash.into(),
        },
        epoch: 2,
        digest: "d".into(),
    }
}

#[test]
fn end_to_end_spaces_take_only_ciphertext_and_control_events(
) -> Result<(), Box<dyn std::error::Error>> {
    let a = Author::root(Signer::generate()?);
    let seen = Some(Seen {
        seq: 3,
        hash: "h3".into(),
    });
    let f = e2e_facts(Role::Member);
    assert_eq!(
        admit(&env(&a, seen.clone(), msg())?, &f)
            .unwrap_err()
            .reason,
        "this space is end-to-end encrypted; seal the event"
    );
    // Commits and Welcomes carry real MLS framing the relay reads (one commit per
    // epoch): `log_store`'s `one commit per epoch, and each welcome follows its commit`
    // drives them with real groups.
    assert_eq!(
        admit(&sealed(&a, SealedKind::Application)?, &f).unwrap(),
        Effect::Nothing
    );
    assert_eq!(
        admit(&sealed(&a, SealedKind::Unspecified)?, &f)
            .unwrap_err()
            .reason,
        "sealed event of an unknown kind"
    );
    let shares = EventBody::ProfileKeyShared {
        version: 1,
        shares: vec![],
    };
    assert_eq!(admit(&env(&a, seen, shares)?, &f).unwrap(), Effect::Nothing);
    // Ciphertext still needs a member: the relay authorizes on the clear framing.
    let stranger = Facts {
        author_role: None,
        ..e2e_facts(Role::Member)
    };
    assert_eq!(
        admit(&sealed(&a, SealedKind::Application)?, &stranger)
            .unwrap_err()
            .reason,
        "not a member of this space"
    );
    Ok(())
}

#[test]
fn privacy_only_goes_up() -> Result<(), Box<dyn std::error::Error>> {
    let a = Author::root(Signer::generate()?);
    let seen = Some(Seen {
        seq: 3,
        hash: "h3".into(),
    });
    let up = |f: &Facts| admit(&env(&a, seen.clone(), EventBody::SpaceEncrypted)?, f);
    let closed = |kind, role| Facts {
        kind: Some(kind),
        privacy: Some(Privacy::Closed),
        ..member_facts(role)
    };
    assert_eq!(
        up(&closed(SpaceKind::Group, Role::Admin)).unwrap(),
        Effect::Encrypt
    );
    assert_eq!(
        up(&closed(SpaceKind::Direct, Role::Member)).unwrap(),
        Effect::Encrypt
    );
    assert_eq!(
        up(&closed(SpaceKind::Group, Role::Member))
            .unwrap_err()
            .reason,
        "only owners and admins turn on encryption"
    );
    assert_eq!(
        up(&closed(SpaceKind::Community, Role::Owner))
            .unwrap_err()
            .reason,
        "only chats and groups become end-to-end"
    );
    assert_eq!(
        up(&e2e_facts(Role::Owner)).unwrap_err().reason,
        "already end-to-end"
    );
    let public = Facts {
        privacy: Some(Privacy::Public),
        ..member_facts(Role::Owner)
    };
    assert_eq!(
        up(&public).unwrap_err().reason,
        "public spaces stay readable"
    );
    Ok(())
}

#[test]
fn spaces_without_a_group_refuse_ciphertext_and_checkpoints(
) -> Result<(), Box<dyn std::error::Error>> {
    let a = Author::root(Signer::generate()?);
    let f = member_facts(Role::Member);
    assert_eq!(
        admit(&sealed(&a, SealedKind::Application)?, &f)
            .unwrap_err()
            .reason,
        "only end-to-end spaces take sealed events"
    );
    let seen = Some(Seen {
        seq: 3,
        hash: "h3".into(),
    });
    let f = Facts {
        upto_hash: Some("h3".into()),
        ..member_facts(Role::Member)
    };
    assert_eq!(
        admit(&env(&a, seen, checkpoint(3, "h3"))?, &f)
            .unwrap_err()
            .reason,
        "checkpoints are for end-to-end spaces"
    );
    Ok(())
}

#[test]
fn a_checkpoint_must_name_this_relays_history() -> Result<(), Box<dyn std::error::Error>> {
    let a = Author::root(Signer::generate()?);
    let seen = Some(Seen {
        seq: 3,
        hash: "h3".into(),
    });
    let at = |upto_hash: Option<&str>| Facts {
        upto_hash: upto_hash.map(Into::into),
        ..e2e_facts(Role::Reader)
    };
    // Readers are group members too, so they checkpoint.
    assert_eq!(
        admit(
            &env(&a, seen.clone(), checkpoint(2, "h2"))?,
            &at(Some("h2"))
        )
        .unwrap(),
        Effect::Checkpoint {
            device: String::new(),
            upto: 2
        }
    );
    for (body, facts) in [
        (checkpoint(2, "h2"), at(Some("other"))),
        (checkpoint(9, "h9"), at(None)),
    ] {
        assert_eq!(
            admit(&env(&a, seen.clone(), body)?, &facts)
                .unwrap_err()
                .reason,
            "checkpoint names a history this relay doesn't have"
        );
    }
    Ok(())
}

fn msg() -> EventBody {
    EventBody::MessagePosted {
        message: "m".into(),
        text: "oi".into(),
        attaches: None,
        reply: None,
        media: None,
        forwarded: false,
    }
}

#[test]
fn members_write_readers_and_strangers_dont() -> Result<(), Box<dyn std::error::Error>> {
    let a = Author::root(Signer::generate()?);
    let seen = Some(Seen {
        seq: 3,
        hash: "h3".into(),
    });
    assert_eq!(
        admit(&env(&a, seen.clone(), msg())?, &member_facts(Role::Member)).unwrap(),
        Effect::Nothing
    );
    assert_eq!(
        admit(&env(&a, seen.clone(), msg())?, &member_facts(Role::Reader))
            .unwrap_err()
            .reason,
        "readers can't write here"
    );
    let stranger = Facts {
        author_role: None,
        ..member_facts(Role::Member)
    };
    assert_eq!(
        admit(&env(&a, seen, msg())?, &stranger).unwrap_err().reason,
        "not a member of this space"
    );
    Ok(())
}

#[test]
fn supported_membership_bounds_every_new_forwarding_audience(
) -> Result<(), Box<dyn std::error::Error>> {
    let author = Author::root(Signer::generate()?);
    let seen = Some(Seen {
        seq: 3,
        hash: "h3".into(),
    });
    let add = env(
        &author,
        seen.clone(),
        EventBody::MemberAdded {
            identity: "c".repeat(64),
            role: Role::Member,
        },
    )?;
    let mut facts = Facts {
        member_count: MAX_MEMBERS - 1,
        ..member_facts(Role::Owner)
    };
    assert!(matches!(admit(&add, &facts), Ok(Effect::Add { .. })));
    facts.member_count = MAX_MEMBERS;
    assert_eq!(
        admit(&add, &facts).unwrap_err().reason,
        "space member limit reached"
    );
    facts.target_role = Some(Role::Reader);
    assert!(
        matches!(admit(&add, &facts), Ok(Effect::Add { .. })),
        "role changes don't grow the audience"
    );
    let mut remove = env(
        &author,
        seen.clone(),
        EventBody::MemberRemoved {
            identity: "d".repeat(64),
        },
    )?;
    assert!(matches!(admit(&remove, &facts), Ok(Effect::Remove { .. })));
    remove = env(
        &author,
        seen,
        EventBody::MemberRemoved {
            identity: "unbounded".repeat(10_000),
        },
    )?;
    assert_eq!(
        admit(&remove, &facts).unwrap_err().reason,
        "not an identity key"
    );
    facts.member_count = MAX_MEMBERS + 1;
    assert_eq!(
        admit(&add, &facts).unwrap_err().reason,
        "space exceeds the supported member limit"
    );
    Ok(())
}

#[test]
fn a_direct_chat_stays_two_people() -> Result<(), Box<dyn std::error::Error>> {
    let a = Author::root(Signer::generate()?);
    let third = EventBody::MemberAdded {
        identity: "c".repeat(64),
        role: Role::Member,
    };
    let facts = Facts {
        kind: Some(SpaceKind::Direct),
        member_count: 2,
        ..member_facts(Role::Owner)
    };
    let r = admit(
        &env(
            &a,
            Some(Seen {
                seq: 3,
                hash: "h3".into(),
            }),
            third,
        )?,
        &facts,
    );
    assert_eq!(r.unwrap_err().reason, "a direct chat has two people");
    Ok(())
}

#[test]
fn joining_needs_a_matching_invite_and_may_skip_the_causal_link(
) -> Result<(), Box<dyn std::error::Error>> {
    let a = Author::root(Signer::generate()?);
    let join = EventBody::MemberAdded {
        identity: a.identity.clone(),
        role: Role::Member,
    };
    let mut e = env(&a, None, join)?;
    let no_invite = Facts {
        author_role: None,
        ..member_facts(Role::Member)
    };
    assert_eq!(
        admit(&e, &no_invite).unwrap_err().reason,
        "joining needs an invite"
    );
    e.invite = Some("CODE".into());
    let invited = Facts {
        invite: Some(("sp_1".into(), Role::Member)),
        ..no_invite
    };
    assert!(matches!(
        admit(&e, &invited).unwrap(),
        Effect::Add {
            by_invite: true,
            ..
        }
    ));
    let elsewhere = Facts {
        invite: Some(("sp_2".into(), Role::Member)),
        author_role: None,
        ..member_facts(Role::Member)
    };
    assert_eq!(
        admit(&e, &elsewhere).unwrap_err().reason,
        "invite is for another space"
    );
    Ok(())
}

#[test]
fn the_causal_link_must_name_this_history() -> Result<(), Box<dyn std::error::Error>> {
    let a = Author::root(Signer::generate()?);
    let wrong = Some(Seen {
        seq: 3,
        hash: "other".into(),
    });
    assert_eq!(
        admit(&env(&a, wrong, msg())?, &member_facts(Role::Member))
            .unwrap_err()
            .reason,
        "signed on a history this relay doesn't have"
    );
    let ahead = Some(Seen {
        seq: 9,
        hash: "h3".into(),
    });
    assert_eq!(
        admit(&env(&a, ahead, msg())?, &member_facts(Role::Member))
            .unwrap_err()
            .reason,
        "signed on a history this relay doesn't have"
    );
    assert_eq!(
        admit(&env(&a, None, msg())?, &member_facts(Role::Member))
            .unwrap_err()
            .reason,
        "an event must say what its author had seen"
    );
    Ok(())
}
