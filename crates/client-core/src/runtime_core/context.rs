//! Native message context has no caller constructor or serialized live proof.
use super::*;

pub struct ReplyMessage {
    pub(super) author: IdentityId,
    pub(super) source: String,
    pub(super) text: Zeroizing<String>,
}
impl ReplyMessage {
    #[must_use]
    pub fn author(&self) -> &str {
        &self.author
    }
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}
impl fmt::Debug for ReplyMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReplyMessage").finish_non_exhaustive()
    }
}
