//! Narrow local subprocess protocol. No user-controlled authority/configuration.
use serde::{Deserialize, Serialize};

pub(crate) const MAX_INPUT: usize = 16 * 1024;
pub(crate) const MAX_OUTPUT: usize = 8 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Command {
    Stage {
        agent: String,
        space: String,
        source: String,
        prompt: String,
    },
    Advance {
        agent: String,
        space: String,
        request: String,
    },
}
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Progress {
    CatchingUp,
    Review {
        request: String,
        content_hash: String,
    },
    Recovery {
        run: String,
    },
    Delivered {
        run: String,
        item: String,
        sha256: String,
    },
}
impl Command {
    pub(crate) fn validate(&self) -> Result<(), String> {
        fn locator(s: &str) -> bool {
            !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)
        }
        match self {
            Self::Stage {
                agent,
                space,
                source,
                prompt,
            } if locator(agent)
                && locator(space)
                && locator(source)
                && !prompt.trim().is_empty()
                && prompt.len() <= 8192 =>
            {
                Ok(())
            }
            Self::Advance {
                agent,
                space,
                request,
            } if locator(agent) && locator(space) && locator(request) => Ok(()),
            _ => Err("invalid image content command".into()),
        }
    }
}
