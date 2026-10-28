//! The owner-only Saved Messages namespace; no personal-profile state or ledger.
use sha2::{Digest, Sha256};

use crate::{EventBody, Privacy, SpaceKind};

const PREFIX: &str = "sp_saved_";

#[must_use]
pub fn space_id(owner: &str) -> Option<String> {
    if owner.len() != 64
        || !owner
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        return None;
    }
    let mut digest = Sha256::new();
    digest.update(b"zoen.saved-messages.v1\0");
    digest.update(owner.as_bytes());
    Some(format!("{PREFIX}{}", hex::encode(digest.finalize())))
}

#[must_use]
pub fn reserved(space: &str) -> bool {
    space.starts_with(PREFIX)
}

#[must_use]
pub fn owned_by(space: &str, owner: &str) -> bool {
    space_id(owner).as_deref() == Some(space)
}

#[must_use]
pub fn genesis(space: &str, author: &str, body: &EventBody) -> bool {
    owned_by(space, author)
        && matches!(body, EventBody::SpaceCreated {
            title, kind: SpaceKind::Direct, privacy: Privacy::EndToEnd,
        } if title.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_is_identity_bound_and_never_a_title_or_device_key() {
        let owner = "ab".repeat(32);
        let other = "cd".repeat(32);
        let id = space_id(&owner).unwrap();
        assert_eq!(space_id(&owner).as_deref(), Some(id.as_str()));
        assert_ne!(space_id(&other), Some(id.clone()));
        assert!(owned_by(&id, &owner));
        assert!(!owned_by(&id, &other));
        assert!(space_id("Saved Messages").is_none());
        assert!(space_id(&owner.to_uppercase()).is_none());
        let body = EventBody::SpaceCreated {
            title: String::new(),
            kind: SpaceKind::Direct,
            privacy: Privacy::EndToEnd,
        };
        assert!(genesis(&id, &owner, &body));
        assert!(!genesis(&id, &other, &body));
        assert!(!genesis("sp_ordinary", &owner, &body));
    }
}
