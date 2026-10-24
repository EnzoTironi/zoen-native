//! Explicit financial operation. Untagged chat retains the original JSON bytes.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum BoundPrice {
    Chat(PriceProfile),
    Image(ImagePriceProfile),
}
impl From<PriceProfile> for BoundPrice {
    fn from(price: PriceProfile) -> Self {
        Self::Chat(price)
    }
}
impl BoundPrice {
    pub(crate) fn chat(&self) -> Result<&PriceProfile, RuntimeError> {
        match self {
            Self::Chat(price) => Ok(price),
            Self::Image(_) => Err(RuntimeError::InvalidProfile),
        }
    }
    pub(crate) fn image(&self) -> Result<&ImagePriceProfile, RuntimeError> {
        match self {
            Self::Image(price) => Ok(price),
            Self::Chat(_) => Err(RuntimeError::InvalidProfile),
        }
    }
    pub(crate) fn currency(&self) -> &str {
        match self {
            Self::Chat(price) => &price.currency,
            Self::Image(price) => &price.currency,
        }
    }
    pub(crate) fn scale(&self) -> u8 {
        match self {
            Self::Chat(price) => price.scale,
            Self::Image(price) => price.scale,
        }
    }
    pub(crate) fn digest(&self) -> Result<String, RuntimeError> {
        match self {
            Self::Chat(price) => price.digest(),
            Self::Image(price) => price.digest(),
        }
    }
    pub(crate) fn reserve(&self, output: u64) -> Result<i64, RuntimeError> {
        match self {
            Self::Chat(price) => price.reserve(output),
            Self::Image(price) if output == 0 => {
                price.digest()?;
                Ok(price.reservation_units)
            }
            Self::Image(_) => Err(RuntimeError::InvalidProfile),
        }
    }
}
