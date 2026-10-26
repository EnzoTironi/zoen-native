//! Canonical owner lifecycle revision. Empty operation means no reviewed control yet.
#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct Record {
    #[prost(string, tag = "1")]
    pub status: String,
    #[prost(uint64, tag = "2")]
    pub revision: u64,
    #[prost(string, tag = "3")]
    pub operation: String,
    #[prost(string, tag = "4")]
    pub action: String,
    #[prost(string, tag = "5")]
    pub device: String,
}
