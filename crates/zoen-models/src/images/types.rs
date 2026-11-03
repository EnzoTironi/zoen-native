use crate::{AttemptContext, OutputFailure, ProviderEvidence, ProviderReceipt};
use serde::{Deserialize, Serialize};

pub const IMAGE_PROFILE: &str = "zoen-openai-images-png-v1/rig-0.44.0";
pub const MAX_IMAGE_BYTES: u64 = 12 * 1024 * 1024;
/// Budget for all normalized JSON fields except the canonical base64 string.
/// This is a supported-response domain, not a provider cost/output guarantee.
pub const IMAGE_JSON_ENVELOPE_BYTES: u64 = 64 * 1024;
/// Exact canonical base64 size (including padding) plus the JSON envelope.
/// Under a 16 MiB transport bound the largest supported PNG is 12,533,760 bytes.
#[must_use]
pub fn image_response_budget(png_bytes: u64) -> Option<u64> {
    if !(1..=MAX_IMAGE_BYTES).contains(&png_bytes) {
        return None;
    }
    png_bytes
        .checked_add(2)?
        .checked_div(3)?
        .checked_mul(4)?
        .checked_add(IMAGE_JSON_ENVELOPE_BYTES)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageSize {
    #[serde(rename = "1024x1024")]
    Square,
    #[serde(rename = "1536x1024")]
    Landscape,
    #[serde(rename = "1024x1536")]
    Portrait,
}
impl ImageSize {
    #[must_use]
    pub fn dimensions(self) -> (u32, u32) {
        match self {
            Self::Square => (1024, 1024),
            Self::Landscape => (1536, 1024),
            Self::Portrait => (1024, 1536),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageQuality {
    Low,
    Medium,
    High,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageBackground {
    Opaque,
    Transparent,
}

/// Exact output obligation for a host-owned image price profile. No token cap is
/// asserted: `/images/generations` exposes no `max_output_tokens` parameter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageDispatch {
    pub count: u8,
    pub size: ImageSize,
    pub quality: ImageQuality,
    pub background: ImageBackground,
    pub format: String,
    pub max_image_bytes: u64,
}
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageRequest {
    pub context: AttemptContext,
    pub prompt: String,
    pub size: ImageSize,
    pub quality: ImageQuality,
    pub background: ImageBackground,
    pub max_image_bytes: u64,
}
/// Original verified bytes, including provider provenance metadata. No remote URL.
#[derive(Clone, PartialEq, Eq)]
pub struct GeneratedImage {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageTokenDetails {
    pub input_text_tokens: Option<u64>,
    pub input_image_tokens: Option<u64>,
    pub output_text_tokens: Option<u64>,
    pub output_image_tokens: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "counters", rename_all = "snake_case")]
pub enum ImageUsageEvidence {
    Missing,
    Reported(ImageTokenDetails),
    Invalid,
}
#[derive(Clone, PartialEq)]
pub struct ImageAttemptResult {
    pub context: AttemptContext,
    pub request_digest: String,
    pub output: Result<GeneratedImage, OutputFailure>,
    pub receipt: ProviderReceipt,
    pub image_usage: ImageUsageEvidence,
    pub evidence: ProviderEvidence,
}
macro_rules! redacted {
    ($($ty:ty),+) => {$(impl std::fmt::Debug for $ty {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(concat!(stringify!($ty), " { redacted }"))
        }
    })+};
}
redacted!(ImageRequest, GeneratedImage, ImageAttemptResult);
