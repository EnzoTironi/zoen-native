use super::*;

#[test]
fn contextual_hashtag_search_pages_only_exact_authorized_messages_and_reindexes_edits(
) -> Result<(), Box<dyn std::error::Error>> {
    let engine = seeded();
    let space = find_space(&engine, "Paraty com a Marina").id;
    let other = engine
        .spaces()
        .into_iter()
        .find(|chat| chat.id != space)
        .ok_or("fixture requires an independent conversation")?
        .id;
    let first = engine
        .send_message(space.clone(), "**#Release_2026**".into())?
        .id;
    let second = engine
        .send_message(space.clone(), "#release_2026 ship".into())?
        .id;
    for text in [
        "release_2026",
        "#release_2026_candidate",
        "https://example.com/#release_2026",
    ] {
        engine.send_message(space.clone(), text.into())?;
    }
    engine.send_message(other, "#release_2026".into())?;
    let page = engine.search_messages(space.clone(), "#release_2026".into(), None, 1)?;
    assert_eq!(page.hits.len(), 1);
    let cursor = page
        .next_cursor
        .ok_or("two matching messages require a next cursor")?;
    let next = engine.search_messages(
        space.clone(),
        "#release_2026".into(),
        Some(cursor.clone()),
        1,
    )?;
    assert_eq!(next.hits.len(), 1);
    assert!(next.next_cursor.is_none());
    let mut actual = vec![page.hits[0].ref_id.clone(), next.hits[0].ref_id.clone()];
    actual.sort();
    let mut expected = vec![first.clone(), second.clone()];
    expected.sort();
    assert_eq!(actual, expected);
    assert!(page
        .hits
        .iter()
        .chain(&next.hits)
        .all(|hit| hit.space_id.as_deref() == Some(space.as_str())));
    engine.edit_message(space.clone(), first, "#different".into())?;
    assert!(engine
        .search_messages(space.clone(), "#release_2026".into(), Some(cursor), 1)
        .is_err());
    let current = engine.search_messages(space, "#release_2026".into(), None, 10)?;
    assert_eq!(current.hits.len(), 1);
    assert_eq!(current.hits[0].ref_id, second);
    Ok(())
}
