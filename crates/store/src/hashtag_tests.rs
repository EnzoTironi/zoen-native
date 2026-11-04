use crate::{Result, Store};

#[test]
fn hashtags_filter_exact_entities_before_the_page_limit() -> Result<()> {
    let store = Store::in_memory()?;
    for (id, at, text) in [
        ("prefix", 100, "#release_candidate"),
        ("bare", 99, "release"),
        ("fragment", 98, "https://example.com/#release"),
        ("embedded", 97, "word#release"),
        ("first", 10, "Send (#Release), today"),
        ("second", 10, "#release ship it"),
        ("last", 1, "again #release!"),
    ] {
        store.index_put("message", id, "chat", "author", at, "", text)?;
    }
    store.index_put("message", "other", "other", "author", 200, "", "#release")?;
    store.index_put("item", "item", "chat", "author", 200, "", "#release")?;
    let first = store.search_messages_page("#release", "chat", None, 1)?;
    assert_eq!(
        first.iter().map(|r| r.ref_id.as_str()).collect::<Vec<_>>(),
        ["first", "second"]
    );
    let next = store.search_messages_page("#release", "chat", Some((10, "first")), 1)?;
    assert_eq!(
        next.iter().map(|r| r.ref_id.as_str()).collect::<Vec<_>>(),
        ["second", "last"]
    );
    assert!(store
        .search_messages_page("#release", "chat", Some((1, "last")), 1)?
        .is_empty());
    assert_eq!(
        store
            .search_messages_page("release", "chat", None, 100)?
            .len(),
        7
    );
    Ok(())
}

#[test]
fn unicode_hashtags_preserve_marks_underscores_and_literal_identity() -> Result<()> {
    let store = Store::in_memory()?;
    for (id, text) in [
        ("accented", "#AÇÃO_2026"),
        ("plain", "#acao_2026"),
        ("cjk", "#发布"),
        ("escaped", r"\#AÇÃO_2026"),
    ] {
        store.index_put("message", id, "chat", "author", 10, "", text)?;
    }
    for query in ["#ação_2026", "#ac\u{327}a\u{303}o_2026"] {
        let hits = store.search_messages_page(query, "chat", None, 10)?;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].ref_id, "accented");
    }
    assert_eq!(
        store.search_messages_page("#发布", "chat", None, 10)?[0].ref_id,
        "cjk"
    );
    for query in ["#", "#___", "#ação_2026 extra", "#ação_2026 OR *"] {
        assert!(store
            .search_messages_page(query, "chat", None, 10)?
            .is_empty());
    }
    Ok(())
}

#[test]
fn headings_and_mixed_text_keep_existing_keyword_semantics() -> Result<()> {
    let store = Store::in_memory()?;
    store.index_put("message", "heading", "chat", "author", 2, "", "# Heading")?;
    store.index_put(
        "message",
        "tag",
        "chat",
        "author",
        1,
        "",
        "#release ship today",
    )?;
    assert_eq!(
        store.search_messages_page("# Heading", "chat", None, 10)?[0].ref_id,
        "heading"
    );
    assert_eq!(
        store.search_messages_page("#release ship", "chat", None, 10)?[0].ref_id,
        "tag"
    );
    Ok(())
}
