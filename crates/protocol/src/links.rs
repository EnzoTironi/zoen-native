//! Public links, one format for every platform. The apps never build these by hand.
//!
//! - friend: `https://tryzoen.com/@<handle>`
//! - Space invite: `https://tryzoen.com/j/<code>`
//! - agent: `https://tryzoen.com/a/<handle|code>` (PR #76)
//!
//! `experiments::attribution_from_link` parses exactly these shapes back.

pub const BASE: &str = "https://tryzoen.com";

#[must_use]
pub fn friend(handle: &str) -> String {
    format!("{BASE}/@{}", handle.trim().trim_start_matches('@'))
}

#[must_use]
pub fn space(code: &str) -> String {
    format!("{BASE}/j/{code}")
}

#[must_use]
pub fn agent(handle_or_code: &str) -> String {
    format!("{BASE}/a/{handle_or_code}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::experiments::{attribution_from_link, SourceKind};

    #[test]
    fn friend_link_round_trips_through_attribution() {
        let link = friend("@Enzo");
        assert_eq!(link, "https://tryzoen.com/@Enzo");
        let a = attribution_from_link(&link).unwrap();
        assert_eq!(a.kind, SourceKind::Friend);
        assert_eq!(space("AB12"), "https://tryzoen.com/j/AB12");
        assert_eq!(
            attribution_from_link(&space("AB12CD34EF")).unwrap().kind,
            SourceKind::Space
        );
    }
}
