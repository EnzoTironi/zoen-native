//! Negotiated complete-set reads. Pages are transport data, never partial authority.
//! The operational profile is finite: callers needing larger directories must fail
//! explicitly rather than install a prefix or seed omitted runtime cursors.
use prost::Message;
use sha2::{Digest, Sha256};

use crate::{AgentStateRecord, DeviceRecord, Reply};

pub const CAPABILITY: &str = "complete-set-reads-v1";
pub const PAGE_ROWS: usize = 128;
pub const PAGE_BYTES: usize = 128 * 1024;
pub const TOTAL_BYTES: usize = 4 * 1024 * 1024;
pub const DEVICE_ROWS: usize = 4096;
pub const CURSOR_ROWS: usize = 16384;
pub const KEY_BYTES: usize = 128;
pub const DEFINITION_BYTES: usize = 64 * 1024;
pub const PROFILE_EXCEEDED: &str = "complete_read_profile_exceeded";
pub const INVALID: &str = "invalid_complete_read";

#[derive(Clone, Copy, Debug, PartialEq, Eq, prost::Enumeration)]
#[repr(i32)]
pub enum Kind {
    Devices = 0,
    AgentState = 1,
}
impl Kind {
    #[must_use]
    pub fn max_rows(self) -> usize {
        match self {
            Self::Devices => DEVICE_ROWS,
            Self::AgentState => CURSOR_ROWS,
        }
    }
}

/// Empty token begins a new read. Otherwise ordinal must be the next page;
/// cancellation releases resources and does not imply rollback of any heartbeat.
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Request {
    #[prost(enumeration = "Kind", tag = "1")]
    pub kind: i32,
    #[prost(string, tag = "2")]
    pub snapshot: String,
    #[prost(uint32, tag = "3")]
    pub ordinal: u32,
    #[prost(bool, tag = "4")]
    pub cancel: bool,
}
impl Request {
    #[must_use]
    pub fn begin(kind: Kind) -> Self {
        Self {
            kind: kind as i32,
            snapshot: String::new(),
            ordinal: 0,
            cancel: false,
        }
    }
    #[must_use]
    pub fn valid(&self) -> bool {
        Kind::try_from(self.kind).is_ok()
            && (if self.snapshot.is_empty() {
                self.ordinal == 0 && !self.cancel
            } else {
                self.snapshot.len() == 64 && self.snapshot.bytes().all(|b| b.is_ascii_hexdigit())
            })
            && usize::try_from(self.ordinal)
                .is_ok_and(|ordinal| ordinal <= CURSOR_ROWS / PAGE_ROWS + 1)
    }
}
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Device {
    #[prost(string, tag = "1")]
    pub device: String,
    #[prost(bool, tag = "2")]
    pub revoked: bool,
}
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Cursor {
    #[prost(string, tag = "1")]
    pub space: String,
    #[prost(uint64, tag = "2")]
    pub seq: u64,
}
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Header {
    #[prost(string, tag = "1")]
    pub status: String,
    #[prost(string, tag = "2")]
    pub kind: String,
    #[prost(uint32, tag = "3")]
    pub definition_version: u32,
    #[prost(bytes = "vec", tag = "4")]
    pub definition: Vec<u8>,
}
impl Header {
    #[must_use]
    pub fn valid(&self) -> bool {
        matches!(self.status.as_str(), "active" | "paused" | "revoked")
            && matches!(self.kind.as_str(), "" | "Creator" | "User")
            && self.definition.len() <= DEFINITION_BYTES
    }
}
#[derive(Clone, PartialEq, Eq, Message)]
pub struct Page {
    #[prost(enumeration = "Kind", tag = "1")]
    pub kind: i32,
    #[prost(string, tag = "2")]
    pub snapshot: String,
    #[prost(uint32, tag = "3")]
    pub ordinal: u32,
    #[prost(string, tag = "4")]
    pub identity: String,
    #[prost(string, tag = "5")]
    pub device: String,
    #[prost(message, optional, tag = "6")]
    pub header: Option<Header>,
    #[prost(message, repeated, tag = "7")]
    pub devices: Vec<Device>,
    #[prost(message, repeated, tag = "8")]
    pub cursors: Vec<Cursor>,
    #[prost(bool, tag = "9")]
    pub complete: bool,
    #[prost(uint32, tag = "10")]
    pub count: u32,
    #[prost(bytes = "vec", tag = "11")]
    pub digest: Vec<u8>,
}
fn hex_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())
}
impl Page {
    #[must_use]
    pub fn wire_len(&self) -> usize {
        self.encoded_len()
    }
    /// One bounded payload copy; digest commits owner/kind/header/order/count/EOF.
    #[must_use]
    pub fn rolling_digest(&self, previous: &[u8]) -> Vec<u8> {
        let mut payload = self.clone();
        payload.digest.clear();
        let mut hash = Sha256::new();
        hash.update(b"zoen-complete-read-v1\0");
        hash.update(previous);
        hash.update(payload.encode_to_vec());
        hash.finalize().to_vec()
    }
    pub fn valid(&self) -> bool {
        Request {
            kind: self.kind,
            snapshot: self.snapshot.clone(),
            ordinal: self.ordinal,
            cancel: false,
        }
        .valid()
            && !self.snapshot.is_empty()
            && hex_key(&self.identity)
            && hex_key(&self.device)
            && self.digest.len() == 32
            && self.devices.len() + self.cursors.len() <= PAGE_ROWS
            && self.encoded_len() <= PAGE_BYTES
            && self.devices.iter().all(|d| hex_key(&d.device))
            && self
                .cursors
                .iter()
                .all(|c| !c.space.is_empty() && c.space.len() <= KEY_BYTES)
            && self.header.as_ref().is_none_or(Header::valid)
            && match Kind::try_from(self.kind) {
                Ok(Kind::Devices) => self.header.is_none() && self.cursors.is_empty(),
                Ok(Kind::AgentState) => {
                    self.devices.is_empty() && (self.ordinal != 0 || self.header.is_some())
                }
                Err(_) => false,
            }
    }
}

/// Disposable O(N) staging, explicitly bounded by profile row/byte ceilings. Only
/// finish returns a legacy complete result. An error poisons the assembly forever.
pub struct Assembly {
    kind: Kind,
    identity: String,
    device: String,
    snapshot: Option<String>,
    ordinal: u32,
    digest: Vec<u8>,
    bytes: usize,
    last: String,
    header: Option<Header>,
    devices: Vec<DeviceRecord>,
    cursors: Vec<(String, u64)>,
    poisoned: bool,
    complete: bool,
}
impl Assembly {
    #[must_use]
    pub fn new(kind: Kind, identity: String, device: String) -> Self {
        Self {
            kind,
            identity,
            device,
            snapshot: None,
            ordinal: 0,
            digest: Vec::new(),
            bytes: 0,
            last: String::new(),
            header: None,
            devices: Vec::new(),
            cursors: Vec::new(),
            poisoned: false,
            complete: false,
        }
    }
    /// # Errors
    /// Rejects invalid ownership, order, digest or profile bounds and poisons this assembly.
    pub fn accept(&mut self, page: Page) -> Result<Option<Request>, &'static str> {
        if self.poisoned {
            return Err(INVALID);
        }
        let result = self.accept_in(page);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn accept_in(&mut self, page: Page) -> Result<Option<Request>, &'static str> {
        if self.complete
            || !page.valid()
            || page.kind != self.kind as i32
            || page.identity != self.identity
            || page.device != self.device
            || page.ordinal != self.ordinal
            || self.snapshot.as_ref().is_some_and(|s| s != &page.snapshot)
            || (self.ordinal != 0 && page.header.is_some())
            || page.digest != page.rolling_digest(&self.digest)
        {
            return Err(INVALID);
        }
        let count =
            self.devices.len() + self.cursors.len() + page.devices.len() + page.cursors.len();
        self.bytes = self
            .bytes
            .checked_add(page.encoded_len())
            .ok_or(PROFILE_EXCEEDED)?;
        if count > self.kind.max_rows() || self.bytes > TOTAL_BYTES {
            return Err(PROFILE_EXCEEDED);
        }
        if usize::try_from(page.count).ok() != Some(count) {
            return Err(INVALID);
        }
        for key in page
            .devices
            .iter()
            .map(|d| &d.device)
            .chain(page.cursors.iter().map(|c| &c.space))
        {
            if key <= &self.last {
                return Err(INVALID);
            }
            self.last = key.clone();
        }
        self.snapshot = Some(page.snapshot.clone());
        self.ordinal = self.ordinal.checked_add(1).ok_or(PROFILE_EXCEEDED)?;
        self.digest = page.digest;
        if self.ordinal == 1 {
            self.header = page.header;
        }
        self.devices
            .extend(page.devices.into_iter().map(|d| DeviceRecord {
                device: d.device,
                revoked: d.revoked,
            }));
        self.cursors
            .extend(page.cursors.into_iter().map(|c| (c.space, c.seq)));
        self.complete = page.complete;
        Ok((!self.complete).then_some(Request {
            kind: self.kind as i32,
            snapshot: page.snapshot,
            ordinal: self.ordinal,
            cancel: false,
        }))
    }
    /// Release only the accepted transport snapshot; never an account mutation.
    #[must_use]
    pub fn cancel_request(&self) -> Option<Request> {
        if self.complete {
            return None;
        }
        self.snapshot.as_ref().map(|snapshot| Request {
            kind: self.kind as i32,
            snapshot: snapshot.clone(),
            ordinal: self.ordinal,
            cancel: true,
        })
    }
    /// # Errors
    /// Rejects poisoned or incomplete reads and agent-state reads without their header.
    pub fn finish(self) -> Result<Reply, &'static str> {
        if self.poisoned || !self.complete {
            return Err(INVALID);
        }
        match self.kind {
            Kind::Devices => Ok(Reply::Devices(self.devices)),
            Kind::AgentState => {
                let h = self.header.ok_or(INVALID)?;
                Ok(Reply::AgentState(AgentStateRecord {
                    status: h.status,
                    kind: h.kind,
                    definition_version: h.definition_version,
                    definition: h.definition,
                    cursors: self.cursors,
                }))
            }
        }
    }
}
#[cfg(test)]
mod tests;
