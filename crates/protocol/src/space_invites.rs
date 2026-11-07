//! Administrative metadata contains hashes, never recoverable invitation secrets.
#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct Request {
    #[prost(string, tag = "1")]
    pub space: String,
    #[prost(oneof = "Action", tags = "2, 3, 4")]
    pub action: Option<Action>,
}
#[derive(Clone, PartialEq, Eq, prost::Oneof)]
pub enum Action {
    #[prost(string, tag = "2")]
    List(String),
    #[prost(string, tag = "3")]
    Revoke(String),
    #[prost(message, tag = "4")]
    Rename(Rename),
}
#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct Rename {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(string, tag = "2")]
    pub name: String,
}
#[derive(Clone, PartialEq, Eq, prost::Message, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SpaceInviteRecord {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(string, tag = "2")]
    pub name: String,
    #[prost(string, tag = "3")]
    pub creator: String,
    #[prost(string, tag = "4")]
    pub role: String,
    #[prost(int64, tag = "5")]
    pub expires_at_ms: i64,
    #[prost(uint32, tag = "6")]
    pub max_uses: u32,
    #[prost(uint32, tag = "7")]
    pub uses: u32,
    #[prost(bool, tag = "8")]
    pub revoked: bool,
}
#[derive(Clone, PartialEq, Eq, prost::Message, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(deny_unknown_fields)]
pub struct SpaceInvitePage {
    #[prost(message, repeated, tag = "1")]
    pub entries: Vec<SpaceInviteRecord>,
    #[prost(string, tag = "2")]
    pub next: String,
    /// A bounded, durable backfill is still indexing pre-upgrade invitations.
    #[prost(bool, tag = "3")]
    pub indexing: bool,
}
pub const CAPABILITY: &str = "space_invite_management_v1";
#[must_use]
pub fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Request {
    /// # Errors
    /// Refuses missing actions, malformed identifiers and oversized or control-bearing names.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.space.is_empty() || self.space.len() > 256 {
            return Err("invalid invite space");
        }
        match &self.action {
            Some(Action::List(after)) if after.is_empty() || valid_id(after) => Ok(()),
            Some(Action::Revoke(id)) if valid_id(id) => Ok(()),
            Some(Action::Rename(rename))
                if valid_id(&rename.id)
                    && rename.name.len() <= 160
                    && rename.name.chars().count() <= 64
                    && !rename.name.chars().any(char::is_control) =>
            {
                Ok(())
            }
            _ => Err("invalid invite management request"),
        }
    }
}
impl SpaceInvitePage {
    /// # Errors
    /// Refuses incomplete indexing presented as a list, invalid metadata or unordered cursors.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.entries.len() > 50
            || (!self.next.is_empty() && !valid_id(&self.next))
            || (self.indexing && (!self.entries.is_empty() || !self.next.is_empty()))
            || !self.entries.windows(2).all(|pair| pair[0].id < pair[1].id)
        {
            return Err("invalid invite page");
        }
        for entry in &self.entries {
            let rename = Request {
                space: "validation".into(),
                action: Some(Action::Rename(Rename {
                    id: entry.id.clone(),
                    name: entry.name.clone(),
                })),
            };
            rename.validate()?;
            if entry.creator.is_empty()
                || entry.creator.len() > 256
                || !matches!(entry.role.as_str(), "Owner" | "Admin" | "Member" | "Reader")
                || entry.max_uses == 0
                || entry.max_uses > 10000
                || entry.uses > entry.max_uses
                || entry.expires_at_ms < 0
                || entry.revoked != (entry.expires_at_ms == 0)
            {
                return Err("invalid invite metadata");
            }
        }
        if !self.next.is_empty()
            && self
                .entries
                .last()
                .is_none_or(|entry| entry.id != self.next)
        {
            return Err("invalid invite cursor");
        }
        Ok(())
    }
}
