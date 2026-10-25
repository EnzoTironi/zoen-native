//! Complete historical references scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn mutations_search_and_references_survive_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(types::new_id("search-reopen")?);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("search.sqlite").to_string_lossy().into_owned();
    let (space, message, quoted, threaded, me, other) = {
        let engine = Client::open(path.clone(), "pt-BR".into()).unwrap();
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
        let engine = Client::open(path.clone(), "pt-BR".into()).unwrap();
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
        let engine = Client::open(path, "pt-BR".into()).unwrap();
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

    Ok(())
}

pub(super) fn historical_recipe_activity_is_searchable_by_its_exact_entry_after_reopen(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(types::new_id("activity-search")?);
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
        let e = Client::open(path, "en".into()).unwrap();
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

    Ok(())
}

pub(super) fn activity_and_message_pages_preserve_threads_deletion_scope_and_unique_ids() {
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

pub(super) fn activity_search_uses_historical_note_not_current_mutable_file_title() {
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
