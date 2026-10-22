//! Complete conversation edits scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn request_previews_use_the_action_title_and_preserve_message_bodies() {
    let e = seeded();
    let crew = find_space(&e, "Turma do Sábado");
    let title = "Ler a agenda da Lúcia pra achar um horário";
    assert!(e
        .demo_open_request(
            crew.id.clone(),
            "zoen".into(),
            title.into(),
            "Só livre/ocupado".into(),
            "Agenda da Lúcia".into(),
            "third_party_data".into(),
            0,
        )
        .unwrap()
        .is_some());
    let preview = find_space(&e, "Turma do Sábado");
    assert_eq!(preview.last_preview, title);
    assert!(
        preview.last_author.is_none(),
        "a request summary needs no author prefix"
    );
    assert!(
        preview.last_delivery.is_none(),
        "requests carry no outgoing ticks"
    );

    let body = "Pedido: trazer água 😂";
    e.send_message(crew.id, body.into()).unwrap();
    let preview = find_space(&e, "Turma do Sábado");
    assert_eq!(
        preview.last_preview, body,
        "ordinary text is never stripped"
    );
    assert!(preview.last_author.unwrap().is_me);
}

pub(super) fn edit_then_undo_restores_content_and_keeps_history() {
    let e = seeded();
    let plan = paraty_plan(&e);
    let p = plan.plan.clone().unwrap();
    let line = &p.sections[2].lines[0]; // escuna
    let out = e
        .edit_plan_line(
            plan.id.clone(),
            line.id.clone(),
            "Escuna + mergulho".into(),
            30_000,
            e.plan_page(plan.id.clone()).unwrap().content_hash,
        )
        .unwrap();
    assert_eq!(out.item.version, 3);
    assert_eq!(
        out.item.plan.as_ref().unwrap().total_cents,
        138_800 - 18_000 + 30_000
    );
    let reaction = out.reaction.expect("o Financeiro comenta o total");
    assert!(reaction.contains("passou"), "{reaction}");

    let undone = e.undo(out.undo.clone()).unwrap();
    assert_eq!(undone.version, 4, "desfazer é uma versão nova");
    assert_eq!(
        undone.plan.as_ref().unwrap(),
        &p,
        "conteúdo idêntico ao de antes"
    );
    assert!(undone.versions[0].is_undo);
    assert_eq!(undone.versions.len(), 4);

    // Restaurar a v1 (antes da troca do jantar).
    let v1 = e
        .restore_plan_version(
            plan.id.clone(),
            1,
            e.plan_page(plan.id.clone()).unwrap().content_hash,
        )
        .unwrap();
    assert_eq!(v1.plan.unwrap().total_cents, 134_800);
    assert!(e.verify_all().iter().all(|r| r.valid));
}

pub(super) fn inline_replies_quote_and_thread_replies_gather_under_the_root() {
    let e = seeded();
    let paraty = find_space(&e, "Paraty com a Marina");
    let tl = e.timeline(paraty.id.clone()).unwrap();
    let root = tl
        .iter()
        .find(|x| matches!(&x.kind, EntryKind::Message { text, .. } if !text.is_empty()))
        .expect("a message to answer")
        .clone();
    assert!(
        tl.iter()
            .all(|x| x.reply_to.is_none() && x.in_thread.is_none()),
        "seeded messages are plain"
    );

    // Inline: stays in the chat, quoting the root.
    let inline = e
        .send_reply(paraty.id.clone(), "Fechado!".into(), root.id.clone(), false)
        .unwrap();
    let q = inline.reply_to.clone().expect("a quote");
    assert_eq!(q.id, root.id);
    assert_eq!(q.author.id, root.author.id);
    assert!(inline.in_thread.is_none());

    // Thread: two replies, the second aimed at the first, both land under the root.
    let t1 = e
        .send_reply(
            paraty.id.clone(),
            "Eu levo o protetor".into(),
            root.id.clone(),
            true,
        )
        .unwrap();
    assert_eq!(t1.in_thread.as_deref(), Some(root.id.as_str()));
    let t2 = e
        .send_reply(paraty.id.clone(), "E eu a água".into(), t1.id.clone(), true)
        .unwrap();
    assert_eq!(
        t2.in_thread.as_deref(),
        Some(root.id.as_str()),
        "threads stay one level deep"
    );

    let tl = e.timeline(paraty.id.clone()).unwrap();
    let r = tl.iter().find(|x| x.id == root.id).unwrap();
    assert_eq!(r.thread_replies, 2);
    let th = e.thread(paraty.id.clone(), root.id.clone()).unwrap();
    assert_eq!(th.len(), 3);
    assert_eq!(th[0].id, root.id);
    assert_eq!(th[0].thread_replies, 2);
    assert_eq!(th[2].id, t2.id);

    // Only messages can be answered, and the target must exist.
    assert!(e
        .send_reply(paraty.id.clone(), "?".into(), "nope".into(), false)
        .is_err());
    assert!(
        e.verify_all().iter().all(|r| r.valid),
        "replies are signed like any message"
    );
}

pub(super) fn edits_deletes_and_reactions_fold_over_the_original() {
    let dir = std::env::temp_dir().join(format!("client-test-{}", types::new_id("t")));
    std::fs::create_dir_all(&dir).unwrap();
    let e = Client::open(
        dir.join("a.sqlite").to_string_lossy().into(),
        "pt-BR".into(),
    )
    .unwrap();
    e.seed_demo_if_empty().unwrap();
    let spaces = e.spaces();
    let s = spaces
        .iter()
        .find(|s| s.members.len() >= 2)
        .expect("a chat")
        .clone();
    let mine = e.send_message(s.id.clone(), "oi".into()).unwrap();
    let theirs = e
        .timeline(s.id.clone())
        .unwrap()
        .into_iter()
        .find(|t| !t.author.is_me && matches!(t.kind, EntryKind::Message { .. }))
        .expect("someone else's message");
    let find = |id: &str| {
        e.timeline(s.id.clone())
            .unwrap()
            .into_iter()
            .find(|t| t.id == id)
            .unwrap()
    };

    e.edit_message(s.id.clone(), mine.id.clone(), "olá".into())
        .unwrap();
    let m = find(&mine.id);
    assert!(m.edited);
    assert!(matches!(m.kind, EntryKind::Message { ref text, .. } if text == "olá"));
    assert!(
        e.edit_message(s.id.clone(), theirs.id.clone(), "x".into())
            .is_err(),
        "only the author edits"
    );
    assert!(
        e.delete_message(s.id.clone(), theirs.id.clone()).is_err(),
        "only the author deletes"
    );

    e.react(s.id.clone(), theirs.id.clone(), Some("👍".into()))
        .unwrap();
    e.react(s.id.clone(), theirs.id.clone(), Some("❤️".into()))
        .unwrap();
    let t = find(&theirs.id);
    assert_eq!(t.reactions.len(), 1, "one reaction per person");
    assert_eq!(t.reactions[0].emoji, "❤️");
    assert!(t.reactions[0].mine);
    e.react(s.id.clone(), theirs.id.clone(), None).unwrap();
    assert!(find(&theirs.id).reactions.is_empty());

    e.delete_message(s.id.clone(), mine.id.clone()).unwrap();
    let m = find(&mine.id);
    assert!(m.deleted);
    assert!(matches!(m.kind, EntryKind::Message { ref text, .. } if text.is_empty()));
    assert!(
        e.edit_message(s.id.clone(), mine.id.clone(), "volta".into())
            .is_err(),
        "deleted is final"
    );

    let other = spaces.iter().find(|x| x.id != s.id).expect("another chat");
    let f = e
        .forward_message(s.id.clone(), theirs.id.clone(), other.id.clone())
        .unwrap();
    assert!(f.forwarded);
}

pub(super) fn ana_replaces_and_removes_only_her_reaction_while_bias_survives_replay() {
    let path = std::env::temp_dir().join(types::new_id("reactions-replay"));
    let e = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
    e.seed_demo_if_empty().unwrap();
    let s = e
        .spaces()
        .into_iter()
        .find(|s| s.members.len() > 2)
        .unwrap();
    let message = e.send_message(s.id.clone(), "react here".into()).unwrap();
    let (ana, bia) = {
        let mut engine = e.lock();
        let owner = engine.me.clone().unwrap();
        let template = engine.identities[&owner].clone();
        let mut ids = Vec::new();
        for name in ["Ana", "Bia"] {
            let mut person = template.clone();
            person.name = name.into();
            person.handle = format!("reaction_{}", name.to_lowercase());
            let id = engine.create_identity(person, true).unwrap();
            engine
                .append(
                    &s.id,
                    &owner,
                    types::EventBody::MemberAdded {
                        identity: id.clone(),
                        role: types::Role::Member,
                    },
                )
                .unwrap();
            ids.push(id);
        }
        (ids.remove(0), ids.remove(0))
    };
    let react_as = |who: &str, emoji: Option<&str>| {
        e.lock().me = Some(who.into());
        e.react(s.id.clone(), message.id.clone(), emoji.map(str::to_string))
            .unwrap();
    };
    let reactions = || {
        e.timeline(s.id.clone())
            .unwrap()
            .into_iter()
            .find(|t| t.id == message.id)
            .unwrap()
            .reactions
    };
    react_as(&bia, Some("👍"));
    react_as(&ana, Some("👍"));
    let both = reactions();
    assert_eq!(both.len(), 1);
    assert!(both[0].mine);
    assert_eq!(both[0].people.len(), 2);
    assert!(both[0].people.iter().any(|p| p.id == ana));
    assert!(both[0].people.iter().any(|p| p.id == bia));
    react_as(&ana, Some("❤️"));
    let replaced = reactions();
    assert_eq!(replaced.len(), 2);
    let bias = replaced.iter().find(|r| r.emoji == "👍").unwrap();
    assert!(!bias.mine);
    assert_eq!(
        bias.people.iter().map(|p| &p.id).collect::<Vec<_>>(),
        vec![&bia]
    );
    react_as(&ana, None);
    let remaining = reactions();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].emoji, "👍");
    assert!(!remaining[0].mine);
    assert_eq!(remaining[0].people[0].id, bia);
    drop(e);
    let e = Client::open(path.to_string_lossy().into(), "en".into()).unwrap();
    e.lock().me = Some(ana);
    let replayed = e
        .timeline(s.id)
        .unwrap()
        .into_iter()
        .find(|t| t.id == message.id)
        .unwrap()
        .reactions;
    assert_eq!(replayed, remaining);
    drop(e);
    std::fs::remove_file(path).unwrap();
}

pub(super) fn chat_prefs_live_in_my_personal_space_and_last_one_wins() {
    let dir = std::env::temp_dir().join(format!("client-test-{}", types::new_id("t")));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.sqlite").to_string_lossy().to_string();
    let id = {
        let e = Client::open(path.clone(), "pt-BR".into()).unwrap();
        e.seed_demo_if_empty().unwrap();
        let s = e.spaces().into_iter().next().expect("a chat");
        assert!(!s.pinned && !s.archived && !s.muted);
        e.set_chat_prefs(s.id.clone(), true, false, Some(i64::MAX))
            .unwrap();
        e.set_chat_prefs(s.id.clone(), true, true, Some(i64::MAX))
            .unwrap();
        s.id
    };
    // Replayed from the log, like a freshly linked device would.
    let e = Client::open(path, "pt-BR".into()).unwrap();
    let s = e.lock().space_summary(&id).unwrap();
    assert!(s.pinned && s.archived && s.muted);
    e.set_chat_prefs(id.clone(), false, false, Some(1)).unwrap();
    let s = e.lock().space_summary(&id).unwrap();
    assert!(
        !s.pinned && !s.archived && !s.muted,
        "a mute in the past is no mute"
    );
}
