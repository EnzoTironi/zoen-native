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
