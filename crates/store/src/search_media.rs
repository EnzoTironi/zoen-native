//! One media category over the existing canonical search projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMediaKind {
    All,
    Photo,
    Video,
    Audio,
    Voice,
    File,
}
impl SearchMediaKind {
    pub fn from_filter(kinds: &[String]) -> Option<Self> {
        let [kind] = kinds else {
            return None;
        };
        Some(match kind.as_str() {
            "media" => Self::All,
            "media:photo" => Self::Photo,
            "media:video" => Self::Video,
            "media:audio" => Self::Audio,
            "media:voice" => Self::Voice,
            "media:file" => Self::File,
            _ => return None,
        })
    }
    pub fn category(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Photo => Some("photo"),
            Self::Video => Some("video"),
            Self::Audio => Some("audio"),
            Self::Voice => Some("voice"),
            Self::File => Some("file"),
        }
    }
}
