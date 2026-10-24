//! Explicit operator endpoint domain; this never admits a model attempt.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageEndpointPolicy {
    #[default]
    Https,
    /// Controlled local-service QA only. DNS names and non-loopback IPs refuse.
    LoopbackFixture,
}
impl ImageEndpointPolicy {
    pub(super) fn is_https(&self) -> bool {
        *self == Self::Https
    }
    pub(super) fn permits(self, endpoint: &url::Url) -> bool {
        match self {
            Self::Https => endpoint.scheme() == "https",
            Self::LoopbackFixture => {
                endpoint.scheme() == "http"
                    && match endpoint.host() {
                        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                        _ => false,
                    }
            }
        }
    }
    pub(crate) fn gateway(self) -> zoen_models::EndpointPolicy {
        match self {
            Self::Https => zoen_models::EndpointPolicy::Https,
            Self::LoopbackFixture => zoen_models::EndpointPolicy::LoopbackFixture,
        }
    }
}
