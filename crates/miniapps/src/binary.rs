//! The concrete binary formats accepted by the existing App Item contract.
//! This selects validation/metadata only; it never grants access or stores state.
use crate::{deckcraft, gridcraft, wordcraft};

pub mod agent_tools;
pub mod gridcraft_registration;
pub mod wordcraft_registration;
pub(crate) mod resource;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Deckcraft,
    Gridcraft,
    Wordcraft,
}
impl Format {
    pub fn for_id(id: &str) -> Option<Self> {
        match id {
            deckcraft::APP => Some(Self::Deckcraft),
            gridcraft::APP => Some(Self::Gridcraft),
            wordcraft::APP => Some(Self::Wordcraft),
            _ => None,
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Deckcraft => deckcraft::APP,
            Self::Gridcraft => gridcraft::APP,
            Self::Wordcraft => wordcraft::APP,
        }
    }
    pub fn resource(self) -> &'static str {
        match self {
            Self::Deckcraft => deckcraft::RESOURCE,
            Self::Gridcraft => gridcraft::RESOURCE,
            Self::Wordcraft => wordcraft::RESOURCE,
        }
    }
    pub fn mime(self) -> &'static str {
        match self {
            Self::Deckcraft => deckcraft::MIME,
            Self::Gridcraft => gridcraft::MIME,
            Self::Wordcraft => wordcraft::MIME,
        }
    }
    pub fn revision(self) -> &'static str {
        match self {
            Self::Deckcraft => deckcraft::ENGINE_REVISION,
            Self::Gridcraft => gridcraft::ENGINE_REVISION,
            Self::Wordcraft => wordcraft::ENGINE_REVISION,
        }
    }
    pub fn max_bytes(self) -> usize {
        match self {
            Self::Deckcraft => deckcraft::MAX_BYTES,
            Self::Gridcraft => gridcraft::MAX_BYTES,
            Self::Wordcraft => wordcraft::MAX_BYTES,
        }
    }
    pub fn seed(self) -> &'static [u8] {
        match self {
            Self::Deckcraft => deckcraft::registration::EMPTY,
            Self::Gridcraft => gridcraft_registration::EMPTY,
            Self::Wordcraft => wordcraft::EMPTY,
        }
    }
    pub fn available(self) -> bool {
        match self {
            Self::Deckcraft => deckcraft::registration::available(),
            Self::Gridcraft => gridcraft_registration::available(),
            Self::Wordcraft => wordcraft_registration::available(),
        }
    }
    pub fn filename(self) -> &'static str {
        match self {
            Self::Deckcraft => "presentation.deckcraft",
            Self::Gridcraft => "workbook.xlsx",
            Self::Wordcraft => "document.docx",
        }
    }
    pub fn item_prefix(self) -> &'static str {
        match self {
            Self::Deckcraft => "it_deck_",
            Self::Gridcraft => "it_grid_",
            Self::Wordcraft => "it_word_",
        }
    }
    pub fn metadata(self, bytes: &[u8], title: &str) -> Result<String, &'static str> {
        match self {
            Self::Deckcraft => Ok(deckcraft::Document::parse(bytes)?.metadata(title)),
            Self::Gridcraft => Ok(gridcraft::Document::parse(bytes)?.metadata(title)),
            Self::Wordcraft => Ok(wordcraft::Document::parse(bytes)?.metadata(title)),
        }
    }
}
