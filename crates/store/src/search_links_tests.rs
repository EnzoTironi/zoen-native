use crate::{Result, SearchMediaKind, SearchPageFilters, Store};

#[test]
fn links_filter_before_limit_and_compose_with_scope_author_dates_without_requiring_media(
) -> Result<()> {
    let store = Store::in_memory()?;
    store.reset_search_links()?;
    store.reset_search_media()?;
    for i in 0..45 {
        for (space, author, linked, prefix) in [
            ("visible", "alice", true, "link"),
            ("visible", "alice", false, "plain"),
            ("visible", "bob", true, "other"),
            ("private", "alice", true, "hidden"),
        ] {
            let id = format!("{prefix}-{i:02}");
            store.index_put("message", &id, space, author, i, "", "needle")?;
            if linked {
                store.index_search_links(space, &id)?;
            }
        }
    }
    store.index_search_media_kind("visible", "link-20", SearchMediaKind::Photo)?;
    let spaces = ["visible".into()];
    let mut ids = std::collections::BTreeSet::new();
    for offset in [0, 10, 20] {
        let mut rows = store.universal_search_page_filtered(
            "needle",
            &["link".into()],
            &spaces,
            offset,
            10,
            SearchPageFilters {
                period: Some(10..40),
                space: Some("visible"),
                author: Some("alice"),
            },
        )?;
        assert_eq!(rows.len(), if offset < 20 { 11 } else { 10 });
        rows.truncate(10);
        for row in rows {
            assert!(
                row.ref_id.starts_with("link-") && row.author == "alice" && row.space == "visible"
            );
            assert!(ids.insert(row.ref_id));
        }
    }
    assert_eq!(ids.len(), 30);
    assert_eq!(
        store
            .universal_search_page("", &["link".into()], &spaces, 0, 100)?
            .len(),
        90
    );
    assert_eq!(
        store
            .universal_search_page("", &["media".into()], &spaces, 0, 100)?
            .len(),
        1
    );
    Ok(())
}

#[test]
fn failed_link_rebuild_keeps_committed_links_and_generation() -> Result<()> {
    let store = Store::in_memory()?;
    store.index_begin()?;
    store.reset_search_links()?;
    store.index_put("message", "kept", "visible", "alice", 1, "", "needle")?;
    store.index_search_links("visible", "kept")?;
    store.index_commit()?;
    let generation = store.search_generation()?;
    store.conn().execute_batch("CREATE TRIGGER link_failure BEFORE INSERT ON search_links WHEN NEW.message = 'lost' BEGIN SELECT RAISE(ABORT, 'injected'); END;")?;
    store.index_begin()?;
    store.index_clear()?;
    store.reset_search_links()?;
    store.index_put("message", "lost", "visible", "alice", 2, "", "needle")?;
    assert!(store.index_search_links("visible", "lost").is_err());
    store.index_abort();
    assert_eq!(store.search_generation()?, generation);
    let rows = store.universal_search_page("", &["link".into()], &["visible".into()], 0, 10)?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].ref_id, "kept");
    Ok(())
}
