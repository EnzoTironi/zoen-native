//! Prepare a history request before changing its retained retry boundary.
use super::{BTreeMap, PageRequest};

pub(super) fn retain_page<'a>(
    pages: &'a mut BTreeMap<String, PageRequest>,
    space: &str,
    before: u64,
    prepare: impl FnOnce() -> Result<String, String>,
) -> Result<&'a PageRequest, String> {
    let fresh = pages
        .get(space)
        .is_none_or(|pending| pending.answer.is_some() || pending.failure.is_some());
    if fresh {
        let request = PageRequest {
            req: prepare()?,
            before,
            answer: None,
            failure: None,
        };
        pages.insert(space.to_owned(), request);
    }
    pages
        .get(space)
        .ok_or_else(|| "Missing retained history request".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn preparation_failure_cannot_create_or_replace_a_history_boundary() {
        let mut pages = BTreeMap::new();
        assert!(retain_page(
            &mut pages,
            "space",
            20,
            || Err("entropy unavailable".into())
        )
        .is_err());
        assert!(pages.is_empty());
        pages.insert(
            "space".into(),
            PageRequest {
                req: "original".into(),
                before: 100,
                answer: None,
                failure: Some("refused".into()),
            },
        );
        let original = serde_json::to_vec(&pages).unwrap();
        assert!(retain_page(
            &mut pages,
            "space",
            20,
            || Err("entropy unavailable".into())
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&pages).unwrap(), original);
    }

    #[test]
    fn uncertain_request_survives_cold_restore_without_allocating_another_id() {
        let mut pages = BTreeMap::new();
        retain_page(&mut pages, "space", 100, || Ok("original".into())).unwrap();
        let encoded = serde_json::to_vec(&pages).unwrap();
        let mut restored: BTreeMap<String, PageRequest> = serde_json::from_slice(&encoded).unwrap();
        let calls = Cell::new(0);
        let pending = retain_page(&mut restored, "space", 20, || {
            calls.set(calls.get() + 1);
            Err("must not allocate on an uncertain retry".into())
        })
        .unwrap();
        assert_eq!(pending.req, "original");
        assert_eq!(pending.before, 100);
        assert_eq!(calls.get(), 0);
        assert_eq!(serde_json::to_vec(&restored).unwrap(), encoded);
    }
}
