//! A selected message locator is scheduling input, never native authority.
use crate::{id, RuntimeError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplySource {
    pub space: String,
    pub message: String,
    pub author: String,
    pub text_sha256: String,
}
impl ReplySource {
    pub(crate) fn validate(&self) -> Result<(), RuntimeError> {
        if !id(&self.space)
            || !id(&self.message)
            || !id(&self.author)
            || !(self.text_sha256.len() == 64
                && self
                    .text_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
        {
            return Err(RuntimeError::InvalidBinding);
        }
        Ok(())
    }
}
