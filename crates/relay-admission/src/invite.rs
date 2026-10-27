//! Shared policy for a Space invite. Durable lookup/collision/use writes remain atomic in storage.
use types::{Role, SpaceKind};

/// # Errors
/// Refuses inviters without current owner/admin authority and non-owner admin invitations.
pub fn authorize_inviter(mine: Option<Role>, role: Role) -> Result<(), &'static str> {
    if !matches!(mine, Some(Role::Owner | Role::Admin)) {
        return Err("only owners and admins create invites");
    }
    if role <= Role::Admin && mine != Some(Role::Owner) {
        return Err("only the owner invites admins");
    }
    Ok(())
}

/// # Errors
/// Refuses invitations for direct conversations.
pub fn authorize_invite_kind(kind: SpaceKind) -> Result<(), &'static str> {
    if kind == SpaceKind::Direct {
        return Err("direct chats don't take invites");
    }
    Ok(())
}

/// `(max uses, TTL milliseconds)`, exactly the native Space-invite bounds.
#[must_use]
pub fn invite_limits(max_uses: u32, ttl_secs: u64) -> (i64, i64) {
    // Values outside i64 are also above the approved 30-day cap. This is
    // the same total clamp as the unsigned input, without a wrapping cast.
    let ttl = match i64::try_from(ttl_secs) {
        Ok(seconds) => seconds.clamp(60, 30 * 24 * 3600),
        Err(_) => 30 * 24 * 3600,
    };
    (i64::from(max_uses.clamp(1, 10_000)), ttl * 1000)
}
