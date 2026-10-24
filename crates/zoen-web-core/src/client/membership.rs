//! Browser storage adapter for the shared MLS membership planner. Claims are
//! journaled before consuming key packages, and recovery blobs gate publication.
use super::*;
use mls::{
    leaf_name,
    reconciliation::{self, Owed},
};
use protocol::{KeyPackageClaimClock, KeyPackageRecord};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const CLAIM_REQUEST: u64 = 0x8000_0030;
const DEVICES_REQUEST_BASE: u64 = 0x1000_0000_0000;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Journal {
    creates: BTreeMap<String, Vec<String>>,
    claims: BTreeMap<String, Claim>,
    joins: BTreeMap<String, BTreeMap<String, String>>,
    stranded: BTreeMap<String, BTreeSet<String>>,
    welcomes: BTreeMap<String, BTreeSet<String>>,
    pub(super) revoked: BTreeSet<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Claim {
    operation: String,
    device: String,
    epoch: u64,
    intent: Owed,
    targets: Vec<String>,
    records: Option<Vec<KeyPackageRecord>>,
}
#[derive(Default)]
pub(super) struct Network {
    pub(super) clock: Option<KeyPackageClaimClock>,
    claiming: Option<(String, String, i64)>,
    retry: BTreeMap<String, i64>,
    turns: BTreeMap<String, i64>,
    devices_requested: bool,
    devices_request: u64,
    devices_generation: u32,
    read_capability: bool,
    devices_assembly: Option<protocol::complete_reads::Assembly>,
    devices_due: i64,
    pub(super) stuck: BTreeSet<String>,
}
impl Network {
    pub(super) fn restart(&mut self) {
        let clock = self.clock;
        let read_capability = self.read_capability;
        let devices_generation = self.devices_generation;
        *self = Self {
            clock,
            read_capability,
            devices_generation,
            ..Default::default()
        };
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    StartDirect {
        identity_id: String,
    },
    CreateGroup {
        title: String,
        member_ids: Vec<String>,
    },
    CreateCommunity {
        title: String,
    },
    AddMember {
        space_id: String,
        identity_id: String,
    },
    AddAdmin {
        space_id: String,
        identity_id: String,
    },
    RemoveMember {
        space_id: String,
        identity_id: String,
    },
    LeaveSpace {
        space_id: String,
    },
    EncryptChat {
        space_id: String,
    },
    // Internal host wake after a confirmed device revocation.
    RefreshDevices {},
}

mod commands;
mod directory;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod directory_tests;
mod network;
mod planner;
