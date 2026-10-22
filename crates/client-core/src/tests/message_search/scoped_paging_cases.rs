//! Complete scoped paging scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn scoped_pages_are_bounded_complete_and_never_leak_other_conversations() {
    let engine = seeded();
    let space = find_space(&engine, "Paraty com a Marina").id;
    let other = engine
        .spaces()
        .into_iter()
        .find(|candidate| candidate.id != space)
        .unwrap()
        .id;
    let mut expected = Vec::new();
    for _ in 0..47 {
        expected.push(
            engine
                .send_message(space.clone(), "paginaçãoúnica viagem".into())
                .unwrap()
                .id,
        );
    }
    engine
        .send_message(other.clone(), "paginaçãoúnica viagem".into())
        .unwrap();
    let mut cursor = None;
    let mut actual = Vec::new();
    let mut order = Vec::new();
    loop {
        let page = engine
            .search_messages(space.clone(), "paginacaounica via".into(), cursor, 7)
            .unwrap();
        assert!(page.hits.len() <= 7);
        assert!(page
            .hits
            .iter()
            .all(|hit| hit.space_id.as_deref() == Some(space.as_str())));
        order.extend(page.hits.iter().map(|hit| (hit.at_ms, hit.ref_id.clone())));
        actual.extend(page.hits.into_iter().map(|hit| hit.ref_id));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
        assert!(actual.len() <= 47, "cursor must advance");
    }
    assert_eq!(actual.len(), 47);
    assert!(order
        .windows(2)
        .all(|pair| pair[0].0 > pair[1].0 || (pair[0].0 == pair[1].0 && pair[0].1 < pair[1].1)));
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
    assert_eq!(
        engine
            .search_messages(other, "paginacaounica via".into(), None, 7)
            .unwrap()
            .hits
            .len(),
        1
    );
}

pub(super) fn scoped_search_rejects_invalid_stale_cross_query_and_cross_space_cursors() {
    let engine = seeded();
    let space = find_space(&engine, "Paraty com a Marina").id;
    let other = engine
        .spaces()
        .into_iter()
        .find(|candidate| candidate.id != space)
        .unwrap()
        .id;
    for _ in 0..3 {
        engine
            .send_message(space.clone(), "cursorunico".into())
            .unwrap();
    }
    let first = engine
        .search_messages(space.clone(), "cursorunico".into(), None, 1)
        .unwrap();
    let cursor = first.next_cursor.unwrap();
    assert!(engine
        .search_messages(space.clone(), "outro".into(), Some(cursor.clone()), 1)
        .is_err());
    assert!(engine
        .search_messages(other, "cursorunico".into(), Some(cursor.clone()), 1)
        .is_err());
    for limit in [0, 101, u32::MAX] {
        assert!(engine
            .search_messages(space.clone(), "cursorunico".into(), None, limit)
            .is_err());
    }
    assert!(engine
        .search_messages(space.clone(), "x".repeat(513), None, 1)
        .is_err());
    assert!(engine
        .search_messages(
            space.clone(),
            "cursorunico".into(),
            Some("bad json".into()),
            1
        )
        .is_err());
    assert!(engine
        .search_messages(
            space.clone(),
            "cursorunico".into(),
            Some("x".repeat(4097)),
            1
        )
        .is_err());
    assert!(engine
        .search_messages("missing".into(), "cursorunico".into(), None, 1)
        .is_err());
    assert!(engine
        .search_messages(space.clone(), " ! \" : ".into(), None, 1)
        .unwrap()
        .hits
        .is_empty());
    engine
        .edit_message(
            space.clone(),
            first.hits[0].ref_id.clone(),
            "mudoutermounico".into(),
        )
        .unwrap();
    assert!(engine
        .search_messages(space.clone(), "cursorunico".into(), Some(cursor), 1)
        .is_err());
    assert_eq!(
        engine
            .search_messages(space, "cursorunico".into(), None, 10)
            .unwrap()
            .hits
            .len(),
        2
    );
}

pub(super) fn scoped_search_propagates_database_errors_and_checks_membership() {
    let engine = seeded();
    let space = find_space(&engine, "Paraty com a Marina").id;
    engine
        .search_messages(space.clone(), "paraty".into(), None, 1)
        .unwrap();
    {
        let mut core = engine.lock();
        let me = core.me.clone();
        let outsider = core
            .identity_order
            .iter()
            .find(|id| !core.is_member(&space, id))
            .cloned()
            .unwrap();
        core.me = Some(outsider);
        assert!(matches!(
            core.search_messages(&space, "paraty", None, 1),
            Err(CoreError::Forbidden { .. })
        ));
        core.me = me;
        core.store
            .conn()
            .execute_batch("DROP TABLE search")
            .unwrap();
    }
    assert!(
        engine
            .search_messages(space, "paraty".into(), None, 1)
            .is_err(),
        "SQL failure is not empty success"
    );
}
