//! One route authority implementation for both independently anchored profiles.
#![deny(clippy::pedantic)]
use crate::continuity::SharedOutputAnchor;
use crate::managed_images::ImageAnchor;
use crate::{RuntimeAuthority, RuntimeError};
use relay_admission::managed_images::{AnchorReader, Checkpoint};

pub(super) enum OutputAnchor {
    Images(ImageAnchor),
    Shared(SharedOutputAnchor),
}
impl OutputAnchor {
    pub(super) fn reader(&self) -> &AnchorReader {
        match self {
            Self::Images(anchor) => &anchor.reader,
            Self::Shared(anchor) => &anchor.reader,
        }
    }
    pub(super) fn checkpoint(&self) -> &Checkpoint {
        match self {
            Self::Images(anchor) => &anchor.checkpoint,
            Self::Shared(anchor) => &anchor.checkpoint,
        }
    }
    pub(super) fn binding(&self) -> (&str, &str, &str) {
        match self {
            Self::Images(anchor) => (
                &anchor.deployment.namespace,
                &anchor.deployment.witness,
                &anchor.deployment.source,
            ),
            Self::Shared(anchor) => (
                &anchor.deployment.namespace,
                &anchor.deployment.witness,
                &anchor.deployment.source,
            ),
        }
    }
    pub(super) fn check_context(&self, runtime: &RuntimeAuthority) -> Result<(), RuntimeError> {
        let present = match self {
            Self::Images(_) => {
                runtime.managed_images.is_some() && runtime.runtime_continuity.is_none()
            }
            Self::Shared(_) => {
                runtime.runtime_continuity.is_some() && runtime.managed_images.is_none()
            }
        };
        if present {
            Ok(())
        } else {
            Err(RuntimeError::RestoreUnreconciled)
        }
    }
}
