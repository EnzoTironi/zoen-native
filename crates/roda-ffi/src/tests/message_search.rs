use super::*;
use roda_types::{Annotation, EventBody};

fn message_hits(engine: &RodaEngine, term: &str) -> Vec<UniversalHit> {
    engine
        .universal_search(term.into(), vec!["message".into()], 100)
        .unwrap()
        .hits
}

#[test]
fn mutations_search_and_references_survive_reopen() {
    let dir = std::env::temp_dir().join(roda_types::new_id("search-reopen"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("search.sqlite").to_string_lossy().into_owned();
    let (space, message, quoted, threaded, me, other) = {
        let engine = RodaEngine::open(path.clone(), "pt-BR".into()).unwrap();
        engine.seed_demo_if_empty().unwrap();
        let space = find_space(&engine, "Paraty com a Marina").id;
        let message = engine
            .send_message(space.clone(), "antigotermounico".into())
            .unwrap()
            .id;
        let quoted = engine
            .send_reply(
                space.clone(),
                "resposta inline".into(),
                message.clone(),
                false,
            )
            .unwrap()
            .id;
        let threaded = engine
            .send_reply(
                space.clone(),
                "resposta na thread".into(),
                message.clone(),
                true,
            )
            .unwrap()
            .id;
        assert_eq!(message_hits(&engine, "antigotermounico")[0].ref_id, message);
        engine
            .edit_message(space.clone(), message.clone(), "novotermounico".into())
            .unwrap();
        assert!(message_hits(&engine, "antigotermounico").is_empty());
        assert!(engine.search("antigotermounico".into()).is_empty());
        assert_eq!(engine.search("novotermounico".into()).len(), 1);
        assert_eq!(message_hits(&engine, "novotermounico")[0].ref_id, message);
        let quote = engine
            .timeline(space.clone())
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == quoted)
            .unwrap()
            .reply_to
            .unwrap();
        assert_eq!(quote.id, message);
        assert_eq!(quote.text, "novotermounico");
        let me = engine.me().unwrap().id;
        let other = engine
            .timeline(space.clone())
            .unwrap()
            .into_iter()
            .find(|entry| !entry.author.is_me && matches!(entry.kind, EntryKind::Message { .. }))
            .unwrap();
        assert!(engine
            .edit_message(space.clone(), other.id.clone(), "ataquetermounico".into())
            .is_err());
        assert!(engine
            .delete_message(space.clone(), other.id.clone())
            .is_err());
        (space, message, quoted, threaded, me, other.id)
    };
    {
        let engine = RodaEngine::open(path.clone(), "pt-BR".into()).unwrap();
        assert!(message_hits(&engine, "antigotermounico").is_empty());
        assert_eq!(message_hits(&engine, "novotermounico")[0].ref_id, message);
        engine
            .delete_message(space.clone(), message.clone())
            .unwrap();
        assert!(message_hits(&engine, "novotermounico").is_empty());
        assert!(engine
            .edit_message(space.clone(), message.clone(), "ressuscitadotermo".into())
            .is_err());
        // Model authenticated, sequenced events arriving late, bypassing the caller API.
        let mut core = engine.lock();
        core.append(
            &space,
            &me,
            EventBody::MessageAnnotated {
                target: message.clone(),
                note: Annotation::Edit {
                    text: "ressuscitadotermo".into(),
                },
            },
        )
        .unwrap();
        core.append(
            &space,
            &me,
            EventBody::MessageAnnotated {
                target: other.clone(),
                note: Annotation::Edit {
                    text: "ataquetermounico".into(),
                },
            },
        )
        .unwrap();
        core.append(
            &space,
            &me,
            EventBody::MessageAnnotated {
                target: other.clone(),
                note: Annotation::Delete,
            },
        )
        .unwrap();
        assert!(
            !core
                .timeline(&space)
                .unwrap()
                .iter()
                .find(|entry| entry.id == other)
                .unwrap()
                .deleted
        );
    }
    {
        let engine = RodaEngine::open(path, "pt-BR".into()).unwrap();
        for term in [
            "antigotermounico",
            "novotermounico",
            "ressuscitadotermo",
            "ataquetermounico",
        ] {
            assert!(engine.search(term.into()).is_empty());
            assert!(
                message_hits(&engine, term).is_empty(),
                "must not resurrect {term}"
            );
            assert!(engine
                .search_messages(space.clone(), term.into(), None, 10)
                .unwrap()
                .hits
                .is_empty());
        }
        let timeline = engine.timeline(space.clone()).unwrap();
        assert!(
            !timeline
                .iter()
                .find(|entry| entry.id == other)
                .unwrap()
                .deleted
        );
        let deleted = timeline.iter().find(|entry| entry.id == message).unwrap();
        assert!(deleted.deleted);
        let quote = timeline
            .iter()
            .find(|entry| entry.id == quoted)
            .unwrap()
            .reply_to
            .as_ref()
            .unwrap();
        assert_eq!(quote.id, message);
        assert!(quote.text.is_empty());
        let thread = engine.thread(space.clone(), message.clone()).unwrap();
        assert!(thread[0].deleted);
        assert!(thread
            .iter()
            .any(|entry| entry.id == threaded
                && entry.in_thread.as_deref() == Some(message.as_str())));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn scoped_pages_are_bounded_complete_and_never_leak_other_conversations() {
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

#[test]
fn scoped_search_rejects_invalid_stale_cross_query_and_cross_space_cursors() {
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

#[test]
fn scoped_search_propagates_database_errors_and_checks_membership() {
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

fn activity_account(path: &str) -> (Arc<RodaEngine>, Arc<MemVault>) {
    let engine = RodaEngine::open(path.into(), "en".into()).unwrap();
    let vault = Arc::new(MemVault::default());
    engine
        .create_account(
            "Activity author".into(),
            "activity_author".into(),
            "http://127.0.0.1:1".into(),
            vault.clone(),
        )
        .unwrap();
    (engine, vault)
}

fn activity_space(engine: &RodaEngine, title: &str) -> String {
    engine
        .create_group_with(title.into(), vec![], PrivacyDto::Closed)
        .unwrap()
}

fn check_cilantro(engine: &RodaEngine, space: &str, recipe: &str) -> TimelineEntry {
    let result = engine
        .app_call_tool(
            recipe.into(),
            "recipe_check".into(),
            r#"{"id":"i7"}"#.into(),
            false,
        )
        .unwrap();
    assert_eq!(result.status, AppCallStatus::Done);
    engine
        .timeline(space.into())
        .unwrap()
        .into_iter()
        .rev()
        .find(|entry| matches!(&entry.kind, EntryKind::ItemEdited { item_id, note, .. } if item_id == recipe && note.contains("Cilantro")))
        .expect("the real recipe action publishes its visible ingredient note")
}

#[test]
fn historical_recipe_activity_is_searchable_by_its_exact_entry_after_reopen() {
    let dir = std::env::temp_dir().join(roda_types::new_id("activity-search"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("account.sqlite").to_string_lossy().into_owned();
    let (space, activity, vault) = {
        let (e, vault) = activity_account(&path);
        let space = activity_space(&e, "Activity search");
        let recipe = e
            .install_app(space.clone(), "recipe".into(), "{}".into())
            .unwrap();
        assert!(recipe
            .app
            .as_ref()
            .unwrap()
            .view_json
            .contains("ingredients"));
        assert!(
            e.search_messages(space.clone(), "Cilantro".into(), None, 10)
                .unwrap()
                .hits
                .is_empty(),
            "an ingredient hidden in app state is not a conversation note"
        );
        let activity = check_cilantro(&e, &space, &recipe.id);
        assert!(e.timeline(space.clone()).unwrap().iter().all(|entry| !matches!(&entry.kind, EntryKind::Message { text, .. } if text.contains("Cilantro"))));
        for hits in [
            e.search_messages(space.clone(), "Cilantro".into(), None, 10)
                .unwrap()
                .hits,
            message_hits(&e, "Cilantro"),
        ] {
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].ref_id, activity.id);
            assert_eq!(hits[0].kind, "message");
            assert_eq!(hits[0].space_id.as_deref(), Some(space.as_str()));
            assert!(hits[0].snippet.contains("Cilantro"));
        }
        e.app_call_tool(
            recipe.id,
            "recipe_servings".into(),
            r#"{"servings":4}"#.into(),
            false,
        )
        .unwrap();
        assert_eq!(
            e.search_messages(space.clone(), "Cilantro".into(), None, 10)
                .unwrap()
                .hits[0]
                .ref_id,
            activity.id
        );
        assert!(
            e.search_messages(space.clone(), "ingredients".into(), None, 10)
                .unwrap()
                .hits
                .is_empty(),
            "opaque app JSON must not become message search text"
        );
        (space, activity, vault)
    };
    {
        let e = RodaEngine::open(path, "en".into()).unwrap();
        assert!(e.unlock(vault).unwrap());
        let hits = e
            .search_messages(space.clone(), "Cilantro".into(), None, 10)
            .unwrap()
            .hits;
        assert_eq!(
            hits.iter()
                .map(|hit| hit.ref_id.as_str())
                .collect::<Vec<_>>(),
            [activity.id.as_str()]
        );
        assert_eq!(message_hits(&e, "Cilantro")[0].ref_id, activity.id);
        let resolved = e
            .timeline(space)
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == hits[0].ref_id)
            .unwrap();
        assert_eq!(
            resolved.kind, activity.kind,
            "the hit resolves to the actual historical note, not a new synthetic message"
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn activity_and_message_pages_preserve_threads_deletion_scope_and_unique_ids() {
    let (e, _vault) = activity_account(":memory:");
    let space = activity_space(&e, "Mixed activity pages");
    let other = activity_space(&e, "Foreign activity pages");
    let recipe = e
        .install_app(space.clone(), "recipe".into(), "{}".into())
        .unwrap();
    let mut expected = Vec::new();
    for _ in 0..5 {
        expected.push(
            e.send_message(space.clone(), "Cilantro message".into())
                .unwrap()
                .id,
        );
        expected.push(check_cilantro(&e, &space, &recipe.id).id);
    }
    let root = expected[0].clone();
    let thread = e
        .send_reply(space.clone(), "Cilantro thread".into(), root.clone(), true)
        .unwrap();
    expected.push(thread.id.clone());
    let deleted = e
        .send_message(space.clone(), "Cilantro deleted".into())
        .unwrap();
    e.delete_message(space.clone(), deleted.id.clone()).unwrap();
    let foreign = e.send_message(other, "Cilantro foreign".into()).unwrap();
    let mut actual = Vec::new();
    let mut order = Vec::new();
    let mut cursor = None;
    loop {
        let page = e
            .search_messages(space.clone(), "Cilantro".into(), cursor, 3)
            .unwrap();
        assert!(page.hits.len() <= 3);
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
        assert!(
            actual.len() <= expected.len(),
            "pagination must advance without duplicate activity rows"
        );
    }
    assert!(!actual.contains(&deleted.id));
    assert!(!actual.contains(&foreign.id));
    assert!(order
        .windows(2)
        .all(|pair| pair[0].0 > pair[1].0 || (pair[0].0 == pair[1].0 && pair[0].1 < pair[1].1)));
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
    assert_eq!(
        e.thread(space.clone(), root.clone())
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == thread.id)
            .unwrap()
            .in_thread
            .as_deref(),
        Some(root.as_str())
    );
    let old_cursor = e
        .search_messages(space.clone(), "Cilantro".into(), None, 1)
        .unwrap()
        .next_cursor
        .unwrap();
    check_cilantro(&e, &space, &recipe.id);
    assert!(
        e.search_messages(space, "Cilantro".into(), Some(old_cursor), 1)
            .is_err(),
        "a new activity invalidates the prior committed index generation"
    );
}

#[test]
fn activity_search_uses_historical_note_not_current_mutable_file_title() {
    let (e, _vault) = activity_account(":memory:");
    let space = activity_space(&e, "File activity search");
    let file = e
        .file_add(
            space.clone(),
            "".into(),
            "original.txt".into(),
            "text/plain".into(),
            b"first".to_vec(),
            None,
        )
        .unwrap();
    e.file_new_version_typed(
        file.id.clone(),
        b"second".to_vec(),
        None,
        "Historicalactivityneedle".into(),
        "mutabletitleunindexed.txt".into(),
        "text/plain".into(),
    )
    .unwrap();
    let activity = e.timeline(space.clone()).unwrap().into_iter().find(|entry| matches!(&entry.kind, EntryKind::ItemEdited { note, .. } if note == "Historicalactivityneedle")).unwrap();
    assert_eq!(
        e.search_messages(space.clone(), "Historicalactivityneedle".into(), None, 10)
            .unwrap()
            .hits[0]
            .ref_id,
        activity.id
    );
    assert_eq!(
        message_hits(&e, "Historicalactivityneedle")[0].ref_id,
        activity.id
    );
    assert!(e
        .search_messages(space.clone(), "mutabletitleunindexed".into(), None, 10)
        .unwrap()
        .hits
        .is_empty());
    assert!(message_hits(&e, "mutabletitleunindexed").is_empty());
    e.file_new_version_typed(
        file.id,
        b"third".to_vec(),
        None,
        "Another activity".into(),
        "latername.txt".into(),
        "text/plain".into(),
    )
    .unwrap();
    assert_eq!(
        e.search_messages(space, "Historicalactivityneedle".into(), None, 10)
            .unwrap()
            .hits[0]
            .ref_id,
        activity.id
    );
}
