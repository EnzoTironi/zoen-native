use crate::{Result, SearchMediaKind, SearchPageFilters, Store, StoreError};
#[test]
fn media_categories_compose_before_limit_with_scope_author_period_and_pagination() -> Result<()> {
    let store = Store::in_memory()?;
    store.reset_search_media()?;
    for i in 0..43 {
        for (space, author, category, suffix) in [
            ("visible", "owner", SearchMediaKind::Photo, "photo"),
            ("visible", "owner", SearchMediaKind::Video, "video"),
            ("visible", "other", SearchMediaKind::Photo, "foreign"),
            ("private", "owner", SearchMediaKind::Photo, "hidden"),
        ] {
            let id = format!("{suffix}-{i:02}");
            store.index_put("message", &id, space, author, i + 10, "", "needle")?;
            store.index_search_media_kind(space, &id, category)?;
        }
    }
    let kinds = ["media:photo".into()];
    let spaces = ["visible".into()];
    let query = |offset| {
        store.universal_search_page_filtered(
            "needle",
            &kinds,
            &spaces,
            offset,
            10,
            SearchPageFilters {
                period: Some(15..35),
                space: Some("visible"),
                author: Some("owner"),
            },
        )
    };
    let mut first = query(0)?;
    assert_eq!(first.len(), 11);
    first.truncate(10);
    let second = query(10)?;
    assert_eq!(second.len(), 10);
    first.extend(second);
    let ids: std::collections::BTreeSet<_> = first.iter().map(|row| row.ref_id.clone()).collect();
    assert_eq!(ids.len(), 20);
    assert!(first.iter().all(|row| row.ref_id.starts_with("photo-")
        && row.author == "owner"
        && row.space == "visible"
        && (15..35).contains(&row.at_ms)));
    for (filter, expected) in [
        ("media:photo", 2),
        ("media:video", 1),
        ("media:audio", 0),
        ("media:voice", 0),
        ("media:file", 0),
    ] {
        let hits = store.universal_search_page("", &[filter.into()], &spaces, 0, 100)?;
        assert_eq!(hits.len(), expected * 43);
    }
    assert!(matches!(
        store.index_search_media_kind("visible", "bad", SearchMediaKind::All),
        Err(StoreError::InvalidSearchKind)
    ));
    Ok(())
}
#[test]
fn legacy_media_index_migrates_and_a_failed_rebuild_restores_its_previous_projection() -> Result<()>
{
    let store = Store::in_memory()?;
    store.conn().execute_batch("CREATE TABLE search_media(space TEXT NOT NULL, message TEXT NOT NULL, PRIMARY KEY(space, message)) WITHOUT ROWID;")?;
    store.index_begin()?;
    store.reset_search_media()?;
    store.index_put("message", "kept", "visible", "owner", 10, "", "needle")?;
    store.index_search_media_kind("visible", "kept", SearchMediaKind::Voice)?;
    store.index_commit()?;
    let before = store.search_generation()?;
    store.conn().execute_batch("CREATE TRIGGER media_index_failure BEFORE INSERT ON search_media WHEN NEW.message = 'lost' BEGIN SELECT RAISE(ABORT, 'injected'); END;")?;
    store.index_begin()?;
    store.reset_search_media()?;
    store.index_clear()?;
    store.index_put("message", "lost", "visible", "owner", 11, "", "needle")?;
    assert!(store
        .index_search_media_kind("visible", "lost", SearchMediaKind::Photo)
        .is_err());
    store.index_abort();
    assert_eq!(store.search_generation()?, before);
    let rows =
        store.universal_search_page("", &["media:voice".into()], &["visible".into()], 0, 10)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ref_id, "kept");
    assert!(store
        .universal_search_page("", &["media:photo".into()], &["visible".into()], 0, 10)?
        .is_empty());
    Ok(())
}
