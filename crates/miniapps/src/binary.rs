//! The two concrete binary formats accepted by the existing App Item contract.
//! This selects validation/metadata only; it never grants access or stores state.
use crate::{deckcraft, gridcraft};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Deckcraft,
    Gridcraft,
}
impl Format {
    pub fn for_id(id: &str) -> Option<Self> {
        match id {
            deckcraft::APP => Some(Self::Deckcraft),
            gridcraft::APP => Some(Self::Gridcraft),
            _ => None,
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Deckcraft => deckcraft::APP,
            Self::Gridcraft => gridcraft::APP,
        }
    }
    pub fn resource(self) -> &'static str {
        match self {
            Self::Deckcraft => deckcraft::RESOURCE,
            Self::Gridcraft => gridcraft::RESOURCE,
        }
    }
    pub fn mime(self) -> &'static str {
        match self {
            Self::Deckcraft => deckcraft::MIME,
            Self::Gridcraft => gridcraft::MIME,
        }
    }
    pub fn revision(self) -> &'static str {
        match self {
            Self::Deckcraft => deckcraft::ENGINE_REVISION,
            Self::Gridcraft => gridcraft::ENGINE_REVISION,
        }
    }
    pub fn max_bytes(self) -> usize {
        match self {
            Self::Deckcraft => deckcraft::MAX_BYTES,
            Self::Gridcraft => gridcraft::MAX_BYTES,
        }
    }
    pub fn filename(self) -> &'static str {
        match self {
            Self::Deckcraft => "presentation.deckcraft",
            Self::Gridcraft => "workbook.xlsx",
        }
    }
    pub fn item_prefix(self) -> &'static str {
        match self {
            Self::Deckcraft => "it_deck_",
            Self::Gridcraft => "it_grid_",
        }
    }
    pub fn metadata(self, bytes: &[u8], title: &str) -> Result<String, &'static str> {
        match self {
            Self::Deckcraft => Ok(deckcraft::Document::parse(bytes)?.metadata(title)),
            Self::Gridcraft => Ok(gridcraft::Document::parse(bytes)?.metadata(title)),
        }
    }
}
