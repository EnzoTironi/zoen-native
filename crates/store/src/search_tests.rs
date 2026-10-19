use super::*;

#[test]
fn search_ties_have_a_stable_reference_order() {
    let store = Store::in_memory().unwrap();
    for id in ["z-last", "a-first"] {
        store
            .index_put("message", id, "chat", "author", 10, "", "agulha")
            .unwrap();
    }
    let hits = store.search("agulha", "message", 10).unwrap();
    assert_eq!(
        hits.iter()
            .map(|hit| hit.ref_id.as_str())
            .collect::<Vec<_>>(),
        vec!["a-first", "z-last"]
    );
}

#[test]
fn scoped_keyset_handles_equal_timestamps_and_filters_before_limiting() {
    let store = Store::in_memory().unwrap();
    for (space, id, at_ms) in [
        ("elsewhere", "outside", 99),
        ("chat", "z", 10),
        ("chat", "a", 10),
        ("chat", "older", 5),
    ] {
        store
            .index_put("message", id, space, "author", at_ms, "", "ação única")
            .unwrap();
    }
    store
        .index_put(
            "item",
            "not-a-message",
            "chat",
            "author",
            100,
            "",
            "ação única",
        )
        .unwrap();
    let first = store
        .search_messages_page("acao uni", "chat", None, 1)
        .unwrap();
    assert_eq!(
        first
            .iter()
            .map(|row| row.ref_id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "z"],
        "one hit plus bounded lookahead"
    );
    let second = store
        .search_messages_page("acao uni", "chat", Some((10, "a")), 1)
        .unwrap();
    assert_eq!(
        second
            .iter()
            .map(|row| row.ref_id.as_str())
            .collect::<Vec<_>>(),
        vec!["z", "older"]
    );
    let third = store
        .search_messages_page("acao uni", "chat", Some((10, "z")), 1)
        .unwrap();
    assert_eq!(third.len(), 1);
    assert_eq!(third[0].ref_id, "older");
    assert!(store
        .search_messages_page("acao uni", "chat", Some((5, "older")), 1)
        .unwrap()
        .is_empty());
    assert!(store
        .search_messages_page("\" OR * :", "chat", None, 1)
        .unwrap()
        .is_empty());
}

#[test]
fn search_generation_is_committed_and_rebuild_abort_restores_content() {
    let store = Store::in_memory().unwrap();
    assert_eq!(store.search_generation().unwrap(), 0);
    store.index_begin().unwrap();
    store
        .index_put("message", "kept", "chat", "author", 1, "", "preservado")
        .unwrap();
    store.index_commit().unwrap();
    assert_eq!(store.search_generation().unwrap(), 1);
    store.index_begin().unwrap();
    store.index_clear().unwrap();
    store
        .index_put("message", "lost", "chat", "author", 2, "", "perdido")
        .unwrap();
    store.index_abort();
    assert_eq!(store.search_generation().unwrap(), 1);
    assert_eq!(
        store.search("preservado", "message", 1).unwrap()[0].ref_id,
        "kept"
    );
    assert!(store.search("perdido", "message", 1).unwrap().is_empty());
    store.index_begin().unwrap();
    store.index_commit().unwrap();
    assert_eq!(store.search_generation().unwrap(), 2);
}

#[test]
fn scoped_search_rejects_unbounded_limits_and_propagates_sql_failure() {
    let store = Store::in_memory().unwrap();
    for limit in [0, 101, u32::MAX] {
        assert!(matches!(
            store.search_messages_page("needle", "chat", None, limit),
            Err(StoreError::InvalidSearchLimit)
        ));
    }
    store.conn().execute_batch("DROP TABLE search").unwrap();
    assert!(matches!(
        store.search_messages_page("needle", "chat", None, 10),
        Err(StoreError::Sqlite(_))
    ));
}
