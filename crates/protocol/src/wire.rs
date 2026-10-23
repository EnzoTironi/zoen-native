//! The protobuf schema of Zoen Sync v2 (`zoen.sync.v2`) and its mapping to the domain
//! frames. The schema lives here, in Rust, so the types and the wire can't drift; field
//! numbers are append-only and never reused. Decoders ignore unknown fields, which is how
//! an older peer tolerates a newer one.

use prost::Message;
use types::{Identity, IdentityKind, Role};

use crate::{
    AgentAction, AgentRequest, AgentStateRecord, AgreementKeyRecord, ClientFrame, Cursor,
    DeviceCertificate, DeviceRecord, DeviceSigned, Envelope, EphemeralKind, ErrorCode,
    InviteCreated, InvitePreview, KeyPackageRecord, Op, Reply, SealedProfile, Sequenced,
    ServerFrame,
};

/// Why a frame didn't decode.
#[derive(Debug, PartialEq, Eq)]
pub struct DecodeError(pub String);

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bad frame: {}", self.0)
    }
}

fn bad(what: &str) -> DecodeError {
    DecodeError(what.to_string())
}

mod schema;
pub use schema::*;
mod mapping;
use mapping::*;
mod client_frames;
mod server_frames;
#[cfg(test)]
mod tests;
