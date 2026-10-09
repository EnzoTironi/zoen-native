//! Who may append what to a Space: pure rules over facts read inside the append
//! transaction, shared by every `LogStore` backend.

use roda_log::content::SealedKind;
use roda_proto::Envelope;
use roda_types::{EventBody, Privacy, Role, SpaceKind};

use super::Reject;

/// What the append transaction read before deciding.
#[derive(Debug, Default)]
pub struct Facts {
    /// `(seq, hash)` of the newest event, `None` for a Space that doesn't exist yet.
    pub head: Option<(u64, String)>,
    pub kind: Option<SpaceKind>,
    pub author_role: Option<Role>,
    /// Role of the identity a `MemberAdded`/`MemberRemoved` names.
    pub target_role: Option<Role>,
    pub member_count: u32,
    /// Hash stored at the envelope's `seen.seq`, when that seq exists.
    pub seen_hash: Option<String>,
    /// The invite the envelope carries: its Space and granted role, if still redeemable.
    pub invite: Option<(String, Role)>,
    /// Whether the identity a `MemberAdded` names is registered (checked in the directory).
    pub target_known: bool,
    pub privacy: Option<Privacy>,
    /// Hash stored at a `Checkpoint`'s `upto.seq`, when that seq exists.
    pub upto_hash: Option<String>,
    /// The MLS epoch the next commit must be made at, and the device whose commit took the
    /// last one. `None` for a group older than this record: its next commit sets it.
    pub mls: Option<(u64, String)>,
}

/// What an admitted envelope changes besides the log.
#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    Create {
        kind: SpaceKind,
        privacy: Privacy,
    },
    Add {
        identity: String,
        role: Role,
        by_invite: bool,
    },
    Remove {
        identity: String,
    },
    /// The Space becomes end-to-end (ADR 0027).
    Encrypt,
    /// An MLS commit took `epoch`; the next must be made at `epoch + 1` (ADR 0026).
    Commit {
        epoch: u64,
        device: String,
    },
    Nothing,
}

pub fn admit(env: &Envelope, f: &Facts) -> Result<Effect, Reject> {
    let body = env.body();
    let author = env.author();
    let effect = match (&f.head, body.clone()) {
        (None, Some(EventBody::SpaceCreated { kind, privacy, .. })) => {
            if kind == SpaceKind::Personal {
                return Err(Reject::no("personal spaces stay on the device"));
            }
            Effect::Create { kind, privacy }
        }
        (None, _) => return Err(Reject::no("unknown space")),
        (Some(_), Some(EventBody::SpaceCreated { .. })) => {
            return Err(Reject::no("space already exists"))
        }
        (Some(_), Some(EventBody::MemberAdded { identity, role })) => {
            if !f.target_known {
                return Err(Reject::no("that person isn't on Zoen yet"));
            }
            if identity == author {
                let Some((space, granted)) = &f.invite else {
                    return Err(Reject::no(if env.invite.is_some() {
                        "invite expired or already used"
                    } else {
                        "joining needs an invite"
                    }));
                };
                if space != env.space() {
                    return Err(Reject::no("invite is for another space"));
                }
                if role < *granted {
                    return Err(Reject::no("invite doesn't grant that role"));
                }
                if f.author_role.is_some() {
                    return Err(Reject::no("already a member"));
                }
                Effect::Add {
                    identity,
                    role,
                    by_invite: true,
                }
            } else {
                match f.author_role {
                    Some(Role::Owner) => {}
                    Some(Role::Admin) if role != Role::Owner => {}
                    _ => return Err(Reject::no("only owners and admins add members")),
                }
                if f.kind == Some(SpaceKind::Direct)
                    && f.member_count >= 2
                    && f.target_role.is_none()
                {
                    return Err(Reject::no("a direct chat has two people"));
                }
                Effect::Add {
                    identity,
                    role,
                    by_invite: false,
                }
            }
        }
        (Some(_), Some(EventBody::MemberRemoved { identity })) => {
            let allowed = identity == author
                || matches!(f.author_role, Some(Role::Owner))
                || (matches!(f.author_role, Some(Role::Admin))
                    && !matches!(f.target_role, Some(Role::Owner)));
            if !allowed {
                return Err(Reject::no("only owners and admins remove members"));
            }
            Effect::Remove { identity }
        }
        (Some(_), Some(EventBody::SpaceEncrypted)) => {
            if !matches!(f.kind, Some(SpaceKind::Direct | SpaceKind::Group)) {
                return Err(Reject::no("only chats and groups become end-to-end"));
            }
            match f.privacy {
                Some(Privacy::Closed) => {}
                Some(Privacy::EndToEnd) => return Err(Reject::no("already end-to-end")),
                _ => return Err(Reject::no("public spaces stay readable")),
            }
            // In a direct chat either person may; in a group, whoever runs it.
            let allowed = match f.author_role {
                Some(Role::Owner | Role::Admin) => true,
                Some(Role::Member) => f.kind == Some(SpaceKind::Direct),
                _ => false,
            };
            if !allowed {
                return Err(Reject::no("only owners and admins turn on encryption"));
            }
            Effect::Encrypt
        }
        (Some(_), Some(EventBody::Checkpoint { upto, .. })) => {
            if f.author_role.is_none() {
                return Err(Reject::no("not a member of this space"));
            }
            let on_chain = f.head.as_ref().is_some_and(|(head, _)| upto.seq <= *head)
                && f.upto_hash.as_deref() == Some(upto.hash.as_str());
            if !on_chain {
                return Err(Reject::no(
                    "checkpoint names a history this relay doesn't have",
                ));
            }
            Effect::Nothing
        }
        (Some(_), _) => match f.author_role {
            None => return Err(Reject::no("not a member of this space")),
            Some(Role::Reader) => return Err(Reject::no("readers can't write here")),
            Some(_) => Effect::Nothing,
        },
    };
    // Who may write comes first: a non-member learns nothing about the Space.
    if f.head.is_some() {
        check_privacy(env, body.as_ref(), f.privacy)?;
    }
    let effect = check_handshake(env, f).map(|e| e.unwrap_or(effect))?;
    check_causal_link(env, f, &effect)?;
    Ok(effect)
}

/// One commit per epoch, in log order: the relay reads the epoch from the clear framing and
/// refuses a commit that lost the race, so members never see two. A Welcome belongs to the
/// last commit, which must be from the same device; after a refused commit there is none.
fn check_handshake(env: &Envelope, f: &Facts) -> Result<Option<Effect>, Reject> {
    let Some((kind, data)) = env.sealed_data() else {
        return Ok(None);
    };
    let device = env.device().unwrap_or(env.author()).to_string();
    match kind {
        SealedKind::Commit => {
            let epoch = roda_mls::commit_epoch(data)
                .ok_or_else(|| Reject::no("a sealed commit that isn't an MLS commit"))?;
            if f.mls.as_ref().is_some_and(|(next, _)| *next != epoch) {
                return Err(Reject::no(roda_proto::STALE_COMMIT));
            }
            Ok(Some(Effect::Commit { epoch, device }))
        }
        SealedKind::Welcome => match &f.mls {
            Some((_, by)) if *by == device => Ok(None),
            _ => Err(Reject::no("a welcome follows its own commit")),
        },
        _ => Ok(None),
    }
}

/// An end-to-end Space takes ciphertext plus the few clear events the relay must read to
/// order and authorize it; anything else in the clear would leak what MLS hides. Other
/// Spaces have no group, so ciphertext there is refused.
fn check_privacy(
    env: &Envelope,
    body: Option<&EventBody>,
    privacy: Option<Privacy>,
) -> Result<(), Reject> {
    let e2e = privacy == Some(Privacy::EndToEnd);
    match (env.sealed_kind(), body) {
        (Some(SealedKind::Unspecified), _) => Err(Reject::no("sealed event of an unknown kind")),
        (Some(_), _) if !e2e => Err(Reject::no("only end-to-end spaces take sealed events")),
        (Some(_), _) => Ok(()),
        (None, Some(EventBody::Checkpoint { .. })) if !e2e => {
            Err(Reject::no("checkpoints are for end-to-end spaces"))
        }
        (
            None,
            Some(
                EventBody::MemberAdded { .. }
                | EventBody::MemberRemoved { .. }
                | EventBody::ProfileKeyShared { .. }
                | EventBody::SpaceEncrypted
                | EventBody::Checkpoint { .. },
            ),
        ) => Ok(()),
        (None, _) if e2e => Err(Reject::no(roda_proto::SEAL_REQUIRED)),
        (None, _) => Ok(()),
    }
}

fn check_causal_link(env: &Envelope, f: &Facts, effect: &Effect) -> Result<(), Reject> {
    match env.seen() {
        Some(seen) => {
            let in_range = f.head.as_ref().is_some_and(|(head, _)| seen.seq <= *head);
            if !in_range || f.seen_hash.as_deref() != Some(seen.hash.as_str()) {
                return Err(Reject::no("signed on a history this relay doesn't have"));
            }
            Ok(())
        }
        None => match effect {
            Effect::Create { .. }
            | Effect::Add {
                by_invite: true, ..
            } => Ok(()),
            _ => Err(Reject::no("an event must say what its author had seen")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use roda_log::{Author, Signer};
    use roda_types::Seen;

    fn env(author: &Author, seen: Option<Seen>, body: EventBody) -> Envelope {
        Envelope::plain(&author.sign_event("sp_1", "c", 1, seen, body))
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

    fn sealed(author: &Author, kind: SealedKind) -> Envelope {
        let seen = Seen {
            seq: 3,
            hash: "h3".into(),
        };
        Envelope::sealed(
            author,
            "sp_1",
            "c",
            1,
            Some(&seen),
            roda_proto::Sealed::new(kind, 3, vec![1, 2, 3]),
        )
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
    fn end_to_end_spaces_take_only_ciphertext_and_control_events() {
        let a = Author::root(Signer::generate());
        let seen = Some(Seen {
            seq: 3,
            hash: "h3".into(),
        });
        let f = e2e_facts(Role::Member);
        assert_eq!(
            admit(&env(&a, seen.clone(), msg()), &f).unwrap_err().reason,
            "this space is end-to-end encrypted; seal the event"
        );
        // Commits and Welcomes carry real MLS framing the relay reads (one commit per
        // epoch): `log_store`'s `one commit per epoch, and each welcome follows its commit`
        // drives them with real groups.
        assert_eq!(
            admit(&sealed(&a, SealedKind::Application), &f).unwrap(),
            Effect::Nothing
        );
        assert_eq!(
            admit(&sealed(&a, SealedKind::Unspecified), &f)
                .unwrap_err()
                .reason,
            "sealed event of an unknown kind"
        );
        let shares = EventBody::ProfileKeyShared {
            version: 1,
            shares: vec![],
        };
        assert_eq!(admit(&env(&a, seen, shares), &f).unwrap(), Effect::Nothing);
        // Ciphertext still needs a member: the relay authorizes on the clear framing.
        let stranger = Facts {
            author_role: None,
            ..e2e_facts(Role::Member)
        };
        assert_eq!(
            admit(&sealed(&a, SealedKind::Application), &stranger)
                .unwrap_err()
                .reason,
            "not a member of this space"
        );
    }

    #[test]
    fn privacy_only_goes_up() {
        let a = Author::root(Signer::generate());
        let seen = Some(Seen {
            seq: 3,
            hash: "h3".into(),
        });
        let up = |f: &Facts| admit(&env(&a, seen.clone(), EventBody::SpaceEncrypted), f);
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
    }

    #[test]
    fn spaces_without_a_group_refuse_ciphertext_and_checkpoints() {
        let a = Author::root(Signer::generate());
        let f = member_facts(Role::Member);
        assert_eq!(
            admit(&sealed(&a, SealedKind::Application), &f)
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
            admit(&env(&a, seen, checkpoint(3, "h3")), &f)
                .unwrap_err()
                .reason,
            "checkpoints are for end-to-end spaces"
        );
    }

    #[test]
    fn a_checkpoint_must_name_this_relays_history() {
        let a = Author::root(Signer::generate());
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
            admit(&env(&a, seen.clone(), checkpoint(2, "h2")), &at(Some("h2"))).unwrap(),
            Effect::Nothing
        );
        for (body, facts) in [
            (checkpoint(2, "h2"), at(Some("other"))),
            (checkpoint(9, "h9"), at(None)),
        ] {
            assert_eq!(
                admit(&env(&a, seen.clone(), body), &facts)
                    .unwrap_err()
                    .reason,
                "checkpoint names a history this relay doesn't have"
            );
        }
    }

    fn msg() -> EventBody {
        EventBody::MessagePosted {
            message: "m".into(),
            text: "oi".into(),
            attaches: None,
        }
    }

    #[test]
    fn members_write_readers_and_strangers_dont() {
        let a = Author::root(Signer::generate());
        let seen = Some(Seen {
            seq: 3,
            hash: "h3".into(),
        });
        assert_eq!(
            admit(&env(&a, seen.clone(), msg()), &member_facts(Role::Member)).unwrap(),
            Effect::Nothing
        );
        assert_eq!(
            admit(&env(&a, seen.clone(), msg()), &member_facts(Role::Reader))
                .unwrap_err()
                .reason,
            "readers can't write here"
        );
        let stranger = Facts {
            author_role: None,
            ..member_facts(Role::Member)
        };
        assert_eq!(
            admit(&env(&a, seen, msg()), &stranger).unwrap_err().reason,
            "not a member of this space"
        );
    }

    #[test]
    fn a_direct_chat_stays_two_people() {
        let a = Author::root(Signer::generate());
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
            ),
            &facts,
        );
        assert_eq!(r.unwrap_err().reason, "a direct chat has two people");
    }

    #[test]
    fn joining_needs_a_matching_invite_and_may_skip_the_causal_link() {
        let a = Author::root(Signer::generate());
        let join = EventBody::MemberAdded {
            identity: a.identity.clone(),
            role: Role::Member,
        };
        let mut e = env(&a, None, join);
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
    }

    #[test]
    fn the_causal_link_must_name_this_history() {
        let a = Author::root(Signer::generate());
        let wrong = Some(Seen {
            seq: 3,
            hash: "other".into(),
        });
        assert_eq!(
            admit(&env(&a, wrong, msg()), &member_facts(Role::Member))
                .unwrap_err()
                .reason,
            "signed on a history this relay doesn't have"
        );
        let ahead = Some(Seen {
            seq: 9,
            hash: "h3".into(),
        });
        assert_eq!(
            admit(&env(&a, ahead, msg()), &member_facts(Role::Member))
                .unwrap_err()
                .reason,
            "signed on a history this relay doesn't have"
        );
        assert_eq!(
            admit(&env(&a, None, msg()), &member_facts(Role::Member))
                .unwrap_err()
                .reason,
            "an event must say what its author had seen"
        );
    }
}
