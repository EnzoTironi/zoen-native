use super::*;

#[test]
fn universal_page_ties_exact_boundaries_and_filters_precede_limit() -> Result<()> {
    let store = Store::in_memory()?;
    for i in 0..60 {
        store.index_put(
            "message",
            &format!("{i:03}"),
            "visible",
            "",
            10,
            "",
            "needle",
        )?;
        store.index_put(
            "message",
            &format!("hidden{i:03}"),
            "private",
            "",
            20,
            "",
            "needle",
        )?;
    }
    let kinds = vec!["message".into()];
    let spaces = vec!["visible".into()];
    let mut first = store.universal_search_page("needle", &kinds, &spaces, 0, 30)?;
    assert_eq!(first.len(), 31);
    first.truncate(30);
    let second = store.universal_search_page("needle", &kinds, &spaces, 30, 30)?;
    assert_eq!(second.len(), 30);
    first.extend(second);
    assert_eq!(
        first
            .iter()
            .map(|row| row.ref_id.clone())
            .collect::<Vec<_>>(),
        (0..60).map(|i| format!("{i:03}")).collect::<Vec<_>>()
    );
    assert!(store
        .universal_search_page("needle", &kinds, &spaces, 60, 30)?
        .is_empty());
    assert!(store
        .universal_search_page("\" OR * :", &kinds, &spaces, 0, 30)?
        .is_empty());
    Ok(())
}

#[test]
fn universal_period_is_half_open_and_applies_before_pagination() -> Result<()> {
    let store = Store::in_memory()?;
    for (id, space, at) in [
        ("before", "visible", 9),
        ("start", "visible", 10),
        ("inside", "visible", 19),
        ("end", "visible", 20),
        ("private", "hidden", 18),
    ] {
        store.index_put("message", id, space, "", at, "", "needle")?;
    }
    let kinds = ["message".into()];
    let spaces = ["visible".into()];
    let first =
        store.universal_search_page_in_range("needle", &kinds, &spaces, 0, 1, Some(10..20))?;
    let next =
        store.universal_search_page_in_range("needle", &kinds, &spaces, 1, 1, Some(10..20))?;
    assert_eq!(
        first
            .iter()
            .map(|row| row.ref_id.as_str())
            .collect::<Vec<_>>(),
        ["inside", "start"]
    );
    assert_eq!(
        next.iter()
            .map(|row| row.ref_id.as_str())
            .collect::<Vec<_>>(),
        ["start"]
    );
    Ok(())
}

#[test]
fn browsing_media_without_text_orders_authorized_messages_before_limiting() -> Result<()> {
    let store = Store::in_memory()?;
    store.reset_search_media()?;
    for (id, space, at, media) in [
        ("latest-text", "visible", 30, false),
        ("private-media", "hidden", 29, true),
        ("visible-media", "visible", 20, true),
        ("older-media", "visible", 10, true),
    ] {
        let body = if id == "visible-media" {
            "🎬".repeat(300)
        } else {
            id.to_owned()
        };
        store.index_put("message", id, space, "", at, "", &body)?;
        if media {
            store.index_search_media(space, id)?;
        }
    }
    let kinds = ["media".into()];
    let spaces = ["visible".into()];
    let hits = store.universal_search_page("", &kinds, &spaces, 0, 1)?;
    assert_eq!(
        hits.iter()
            .map(|hit| hit.ref_id.as_str())
            .collect::<Vec<_>>(),
        ["visible-media", "older-media"]
    );
    assert_eq!(hits[0].snippet, format!("{}…", "🎬".repeat(240)));
    assert_eq!(hits[1].snippet, "older-media");
    Ok(())
}

#[test]
fn identity_filters_apply_before_limit_and_exclude_non_message_rows() -> Result<()> {
    let store = Store::in_memory()?;
    store.reset_search_media()?;
    for (kind, id, space, author, at) in [
        ("message", "wanted", "room", "alice", 15),
        ("message", "other-author", "room", "bob", 19),
        ("message", "other-room", "other", "alice", 19),
        ("message", "too-old", "room", "alice", 9),
        ("person", "person-row", "room", "alice", 19),
    ] {
        store.index_put(kind, id, space, author, at, "", "needle")?;
    }
    store.index_search_media("room", "wanted")?;
    let kinds = ["message".into(), "person".into()];
    let media_kinds = ["media".into()];
    let readable = ["room".into(), "other".into()];
    for query in ["needle", ""] {
        for media in [false, true] {
            let result = store.universal_search_page_filtered(
                query,
                if media { &media_kinds } else { &kinds },
                &readable,
                0,
                1,
                SearchPageFilters {
                    period: Some(10..20),
                    space: Some("room"),
                    author: Some("alice"),
                },
            )?;
            assert_eq!(
                result
                    .iter()
                    .map(|row| row.ref_id.as_str())
                    .collect::<Vec<_>>(),
                ["wanted"]
            );
        }
    }
    Ok(())
}
