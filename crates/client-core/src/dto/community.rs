//! Owner-bound disposable views of the existing signed membership projection.
use super::Persona;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum MemberActionState {
    Active,
    Updating,
    Removing,
    Removed,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct SpaceMemberContext {
    pub identity_id: String,
    pub device_id: String,
    pub space_id: String,
    pub space_title: String,
    pub member_id: String,
    pub member: Option<Persona>,
    pub role: Option<String>,
    pub state: MemberActionState,
    pub can_remove: bool,
    /// A derived review fingerprint, not a stored grant or a mutation receipt.
    pub revision: String,
}
