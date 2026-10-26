//! Who may append what to a Space: pure rules over facts read inside the append
//! transaction, shared by every `LogStore` backend.

use event_log::content::SealedKind;
use protocol::Envelope;
use types::{EventBody, Privacy, Role, SpaceKind};

use super::Reject;

/// Current relay/fanout contract. Larger groups require a paginated delivery path.
pub const MAX_MEMBERS: u32 = 5_000;

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
    /// A member device holds everything up to `upto` (ADR 0026: what pruning waits on).
    Checkpoint {
        device: String,
        upto: u64,
    },
    /// One of the author's devices joins the group from this entry: pruning waits for it.
    Hold {
        device: String,
    },
    Nothing,
}

pub fn admit(env: &Envelope, f: &Facts) -> Result<Effect, Reject> {
    if f.member_count > MAX_MEMBERS {
        return Err(Reject::no("space exceeds the supported member limit"));
    }
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
            if !identity_key(&identity) {
                return Err(Reject::no("not an identity key"));
            }
            if f.target_role.is_none() && f.member_count >= MAX_MEMBERS {
                return Err(Reject::no("space member limit reached"));
            }
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
                if !f
                    .author_role
                    .is_some_and(|actor| actor.can_assign(f.target_role, role))
                {
                    return Err(Reject::no(if f.target_role == Some(Role::Owner) {
                        "owner role is protected"
                    } else {
                        "only owners and admins add members"
                    }));
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
            if !identity_key(&identity) {
                return Err(Reject::no("not an identity key"));
            }
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
            Effect::Checkpoint {
                device: env.device().unwrap_or_default().to_string(),
                upto: upto.seq,
            }
        }
        (Some(_), Some(EventBody::DeviceJoining { device })) => {
            if f.author_role.is_none() {
                return Err(Reject::no("not a member of this space"));
            }
            if f.privacy != Some(Privacy::EndToEnd) {
                return Err(Reject::no("devices join groups of end-to-end spaces only"));
            }
            if device.len() != 64 || !device.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(Reject::no("not a device key"));
            }
            Effect::Hold { device }
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

pub fn identity_key(identity: &str) -> bool {
    identity.len() == 64 && identity.bytes().all(|b| b.is_ascii_hexdigit())
}

/// One commit per epoch, in log order: the relay reads the epoch from the clear framing and
/// refuses a commit that lost the race, so members never see two. A Welcome belongs to the
/// last commit, which must be from the same device; after a refused commit there is none.
fn check_handshake(env: &Envelope, f: &Facts) -> Result<Option<Effect>, Reject> {
    let Some((kind, data)) = env.sealed_data() else {
        return Ok(None);
    };
    let device = env.device().unwrap_or(env.author()).to_string();
    if let Some(bytes) = env.recovery() {
        let reference = event_log::recovery::RecoveryRef::parse(bytes)
            .ok_or_else(|| Reject::no("invalid recovery context reference"))?;
        if kind != SealedKind::Commit
            || mls::commit_epoch(data).and_then(|e| e.checked_add(1)) != Some(reference.epoch)
        {
            return Err(Reject::no(
                "recovery context must follow its commit's epoch",
            ));
        }
    }
    match kind {
        SealedKind::Commit => {
            let epoch = mls::commit_epoch(data)
                .ok_or_else(|| Reject::no("a sealed commit that isn't an MLS commit"))?;
            if f.mls.as_ref().is_some_and(|(next, _)| *next != epoch) {
                return Err(Reject::no(protocol::STALE_COMMIT));
            }
            Ok(Some(Effect::Commit { epoch, device }))
        }
        SealedKind::Application => match (&f.mls, mls::application_epoch(data)) {
            (Some((current, _)), Some(epoch)) if epoch != *current => {
                Err(Reject::no(protocol::STALE_SEAL))
            }
            _ => Ok(None),
        },
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
                | EventBody::Checkpoint { .. }
                | EventBody::DeviceJoining { .. },
            ),
        ) => Ok(()),
        (None, _) if e2e => Err(Reject::no(protocol::SEAL_REQUIRED)),
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
mod tests;
