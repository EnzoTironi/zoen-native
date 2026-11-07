//! Provisioning schedules an independently keyed host; it grants no owner consent.
#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct Service {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(string, tag = "2")]
    pub template_sha256: String,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct Availability {
    #[prost(message, optional, tag = "1")]
    pub service: Option<Service>,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct Request {
    #[prost(string, tag = "1")]
    pub operation: String,
    #[prost(message, optional, tag = "2")]
    pub service: Option<Service>,
    #[prost(string, tag = "3")]
    pub name: String,
    #[prost(string, tag = "4")]
    pub handle: String,
    /// Owner-side HPKE recipient. The private key stays in the owner's vault.
    #[prost(string, tag = "5")]
    pub recipient_public_key: String,
}

#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct Record {
    #[prost(message, optional, tag = "1")]
    pub request: Option<Request>,
    #[prost(string, tag = "2")]
    pub owner: String,
    /// queued/offered/registered/expired; registration is not runtime readiness.
    #[prost(string, tag = "3")]
    pub state: String,
    #[prost(string, tag = "4")]
    pub agent: String,
    #[prost(string, tag = "5")]
    pub offer_digest: String,
    #[prost(bytes = "vec", tag = "6")]
    pub sealed_offer: Vec<u8>,
    #[prost(uint64, tag = "7")]
    pub expires_ms: u64,
}

#[must_use]
pub fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Service {
    #[must_use]
    pub fn valid(&self) -> bool {
        digest(&self.id) && digest(&self.template_sha256)
    }
}
impl Request {
    #[must_use]
    pub fn valid(&self) -> bool {
        digest(&self.operation)
            && self.service.as_ref().is_some_and(Service::valid)
            && !self.name.is_empty()
            && self.name.trim() == self.name
            && self.name.chars().count() <= 64
            && !self.name.chars().any(char::is_control)
            && crate::normalize_handle(&self.handle).as_deref() == Some(self.handle.as_str())
            && digest(&self.recipient_public_key)
    }
}

impl Record {
    #[must_use]
    pub fn valid(&self) -> bool {
        if !self.request.as_ref().is_some_and(Request::valid) || !digest(&self.owner) {
            return false;
        }
        match self.state.as_str() {
            "queued" | "rejected" => {
                self.agent.is_empty()
                    && self.offer_digest.is_empty()
                    && self.sealed_offer.is_empty()
                    && self.expires_ms == 0
            }
            "offered" | "registered" | "expired" => {
                digest(&self.agent)
                    && digest(&self.offer_digest)
                    && (40..=16384).contains(&self.sealed_offer.len())
                    && self.expires_ms > 0
            }
            _ => false,
        }
    }
}
