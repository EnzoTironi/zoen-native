#[derive(Clone, PartialEq, prost::Message)]
pub struct PbPushToken {
    #[prost(string, tag = "1")]
    pub platform: String,
    #[prost(string, tag = "2")]
    pub token: String,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct PbVoipConsent {
    #[prost(string, tag = "1")]
    pub space: String,
    #[prost(string, tag = "2")]
    pub peer: String,
    #[prost(bool, tag = "3")]
    pub allowed: bool,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct PbVoipWake {
    #[prost(string, tag = "1")]
    pub space: String,
    #[prost(string, tag = "2")]
    pub peer: String,
    #[prost(uint64, tag = "3")]
    pub seq: u64,
}

#[derive(Clone, PartialEq, prost::Message)]
pub struct PbHello {
    #[prost(uint32, tag = "1")]
    pub protocol: u32,
    #[prost(string, repeated, tag = "2")]
    pub capabilities: Vec<String>,
    #[prost(string, tag = "3")]
    pub identity: String,
    #[prost(string, tag = "4")]
    pub device: String,
    #[prost(string, tag = "5")]
    pub cert: String,
}
