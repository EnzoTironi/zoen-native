//! Append-only wire fields for private Runtime/relay binding.
use prost::Message;

#[derive(Clone, PartialEq, Message)]
pub struct PbRuntimeBindingRequest {
    #[prost(string, tag = "1")]
    pub namespace: String,
    #[prost(bytes = "vec", tag = "2")]
    pub nonce: Vec<u8>,
    #[prost(message, optional, tag = "3")]
    pub managed: Option<PbManagedCheckpoint>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbManagedCheckpoint {
    #[prost(uint64, tag = "1")]
    pub generation: u64,
    #[prost(uint64, tag = "2")]
    pub sequence: u64,
    #[prost(string, tag = "3")]
    pub digest: String,
    #[prost(message, optional, tag = "4")]
    pub source_fence: Option<PbSourceFence>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbRuntimeBindingProof {
    #[prost(string, tag = "1")]
    pub cell: String,
    #[prost(bytes = "vec", tag = "2")]
    pub tag: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PbSourceFence {
    #[prost(string, tag = "1")]
    pub space: String,
    #[prost(uint64, tag = "2")]
    pub sequence: u64,
    #[prost(string, tag = "3")]
    pub hash: String,
    #[prost(string, tag = "4")]
    pub run: String,
    #[prost(string, tag = "5")]
    pub attempt: String,
    #[prost(int64, optional, tag = "6")]
    pub expires_at_ms: Option<i64>,
}
