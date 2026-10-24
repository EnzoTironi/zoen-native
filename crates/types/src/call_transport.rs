//! Bounded WebRTC negotiation carried by the existing authenticated MLS channel.
//! DTLS fingerprints and ICE credentials stay inside the encrypted Space event.
use serde::{Deserialize, Serialize};

pub const MAX_SDP_BYTES: usize = 48 * 1024;
pub const MAX_ICE_BYTES: usize = 4096;
pub const MAX_NEGOTIATION_SIGNALS: usize = 96;
pub const MAX_NEGOTIATION_BYTES: usize = 512 * 1024;

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CallTransportPayload {
    Offer {
        sdp: String,
    },
    Answer {
        sdp: String,
    },
    Ice {
        candidate: String,
        mid: Option<String>,
        line: u16,
    },
}

// SDP and ICE contain transport credentials. Avoid accidental Debug disclosure.
impl std::fmt::Debug for CallTransportPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Offer { .. } => "Offer(<redacted>)",
            Self::Answer { .. } => "Answer(<redacted>)",
            Self::Ice { .. } => "Ice(<redacted>)",
        })
    }
}

impl CallTransportPayload {
    pub fn byte_len(&self) -> usize {
        match self {
            Self::Offer { sdp } | Self::Answer { sdp } => sdp.len(),
            Self::Ice { candidate, mid, .. } => {
                candidate.len() + mid.as_ref().map_or(0, String::len)
            }
        }
    }
    #[must_use]
    pub fn valid_shape(&self) -> bool {
        match self {
            Self::Offer { sdp } | Self::Answer { sdp } => {
                !sdp.is_empty() && sdp.len() <= MAX_SDP_BYTES && !sdp.as_bytes().contains(&0)
            }
            Self::Ice { candidate, mid, .. } => {
                !candidate.is_empty()
                    && candidate.len() <= MAX_ICE_BYTES
                    && !candidate.contains(['\0', '\r', '\n'])
                    && mid.as_ref().is_none_or(|m| {
                        !m.is_empty() && m.len() <= 64 && !m.contains(['\0', '\r', '\n'])
                    })
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CallTransportSignal {
    pub signal_id: String,
    pub call_id: String,
    pub invitation: String,
    pub recipient_device: String,
    pub generation: u8,
    pub payload: CallTransportPayload,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CallTransportRecord {
    pub sender_device: String,
    pub signal: CallTransportSignal,
}
