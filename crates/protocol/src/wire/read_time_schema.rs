use prost::Message;
/// Optional metadata on the unchanged Read wire discriminator.
#[derive(Clone, PartialEq, Message)]
pub struct PbReadTime {
    /// Exclusive previous prefix; older messages get no retroactive date.
    #[prost(uint64, tag = "1")]
    pub after_seq: u64,
    /// Peer device clock, not relay observation time.
    #[prost(int64, tag = "2")]
    pub reported_at_ms: i64,
}
