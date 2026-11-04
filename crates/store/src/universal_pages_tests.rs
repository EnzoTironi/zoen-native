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
