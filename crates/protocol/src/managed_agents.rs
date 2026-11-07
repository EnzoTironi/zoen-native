//! Provisioning schedules an independently keyed host; it grants no owner consent.
#[derive(Clone, PartialEq, Eq, prost::Message, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, PartialEq, Eq, prost::Message, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// `queued/offered/registered/expired/rejected/cancel_requested/cancelled`.
    /// Registration is not runtime readiness.
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
    #[prost(bool, tag = "8")]
    pub onboarding_completed: bool,
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
        if !self.request.as_ref().is_some_and(Request::valid)
            || !digest(&self.owner)
            || self.onboarding_completed && self.state != "registered"
        {
            return false;
        }
        let empty = self.agent.is_empty()
            && self.offer_digest.is_empty()
            && self.sealed_offer.is_empty()
            && self.expires_ms == 0;
        let offered = digest(&self.agent)
            && digest(&self.offer_digest)
            && (40..=16384).contains(&self.sealed_offer.len())
            && self.expires_ms > 0;
        match self.state.as_str() {
            "cancel_requested" | "cancelled" => empty || offered,
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

/// Owner-scoped unfinished onboarding, ordered by the immutable operation key.
#[derive(Clone, PartialEq, Eq, prost::Message)]
pub struct Page {
    #[prost(message, repeated, tag = "1")]
    pub records: Vec<Record>,
    #[prost(string, tag = "2")]
    pub next_operation: String,
}
impl Page {
    #[must_use]
    pub fn valid(&self) -> bool {
        self.records.len() <= 32
            && (self.next_operation.is_empty()
                || self.records.len() == 32
                    && self
                        .records
                        .last()
                        .and_then(|r| r.request.as_ref())
                        .is_some_and(|r| r.operation == self.next_operation))
            && self.records.iter().all(|r| {
                r.valid()
                    && !r.onboarding_completed
                    && matches!(
                        r.state.as_str(),
                        "queued" | "offered" | "cancel_requested" | "registered"
                    )
            })
            && self.records.windows(2).all(|rows| {
                rows[0].owner == rows[1].owner
                    && rows[0]
                        .request
                        .as_ref()
                        .zip(rows[1].request.as_ref())
                        .is_some_and(|(a, b)| a.operation < b.operation)
            })
    }
}
