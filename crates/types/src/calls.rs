//! Version-one call signaling inside the authenticated, MLS-encrypted Space log.
//! A transport connection is deliberately not a durable `connected` assertion.

use crate::IdentityId;
use serde::{Deserialize, Serialize};

/// An invitation may ring for at most one minute; clients and media admission also
/// check the current clock. Signed sender time alone never authorizes live media.
pub const MAX_RING_MS: i64 = 60_000;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CallMedia {
    Audio,
    Video,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CallEndReason {
    Cancelled,
    Declined,
    HungUp,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CallSignal {
    Invite {
        call_id: String,
        callee: IdentityId,
        media: CallMedia,
        expires_at_ms: i64,
    },
    Accept {
        call_id: String,
        invitation: String,
    },
    End {
        call_id: String,
        invitation: String,
        reason: CallEndReason,
    },
}

impl CallSignal {
    pub fn call_id(&self) -> &str {
        match self {
            Self::Invite { call_id, .. }
            | Self::Accept { call_id, .. }
            | Self::End { call_id, .. } => call_id,
        }
    }
}

/// The event's content hash survives relay confirmation; its chain hash does not.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CallRecord {
    pub call_id: String,
    pub invitation: String,
    pub caller: IdentityId,
    pub caller_device: String,
    pub callee: IdentityId,
    pub media: CallMedia,
    pub invited_at_ms: i64,
    pub expires_at_ms: i64,
    pub invitation_confirmed: bool,
    pub accepted_device: Option<String>,
    pub accepted_at_ms: Option<i64>,
    pub acceptance_confirmed: bool,
    pub ended_at_ms: Option<i64>,
    pub end_reason: Option<CallEndReason>,
    /// A membership change retires media; it is not a forged participant hangup.
    pub invalidated: bool,
}

impl CallRecord {
    pub fn ringing_at(&self, now_ms: i64) -> bool {
        !self.terminal()
            && self.accepted_device.is_none()
            && now_ms >= self.invited_at_ms
            && now_ms < self.expires_at_ms
    }
    pub fn terminal(&self) -> bool {
        self.invalidated || self.end_reason.is_some()
    }
}
