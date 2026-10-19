//! Shared policy for a Space invite. Durable lookup/collision/use writes remain atomic in storage.
use roda_types::{Role, SpaceKind};

pub fn authorize_inviter(mine: Option<Role>, role: Role) -> Result<(), &'static str> {
    if !matches!(mine, Some(Role::Owner | Role::Admin)) {
        return Err("only owners and admins create invites");
    }
    if role <= Role::Admin && mine != Some(Role::Owner) {
        return Err("only the owner invites admins");
    }
    Ok(())
}

pub fn authorize_invite_kind(kind: SpaceKind) -> Result<(), &'static str> {
    if kind == SpaceKind::Direct {
        return Err("direct chats don't take invites");
    }
    Ok(())
}

/// `(max uses, TTL milliseconds)`, exactly the native Space-invite bounds.
pub fn invite_limits(max_uses: u32, ttl_secs: u64) -> (i64, i64) {
    (
        max_uses.clamp(1, 10_000) as i64,
        ttl_secs.clamp(60, 30 * 24 * 3600) as i64 * 1000,
    )
}
