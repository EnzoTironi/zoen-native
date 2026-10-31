//! Original whole test bodies, called by their unchanged registered facade.
use super::*;

pub(super) fn browser_plans_preserve_large_cents_and_refuse_stale_writes_and_undo(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, mut log) = document_fixture()?;
    let plan = types::PlanDoc {
        title: "Plano exato".into(),
        summary: "Centavos sem float".into(),
        budget_cents: Some(i64::MAX),
        sections: vec![types::PlanSection {
            title: "Custos".into(),
            lines: vec![types::PlanLine {
                id: "ln_exact".into(),
                text: "Custo grande".into(),
                cost_cents: 9_007_199_254_740_993,
                done: false,
            }],
        }],
    };
    let made = clear(
        &mut log,
        &author,
        EventBody::ItemCreated {
            item: "it_plan".into(),
            kind: types::ItemKind::Plan,
            content: types::ItemContent::Plan(plan),
            origin: "Criado no nativo".into(),
        },
        10,
    )?;
    client.receive(&made, 11).unwrap();
    let read = doc(
        &mut client,
        serde_json::json!({"op":"plan_page","item_id":"it_plan"}),
        12,
    )
    .result;
    assert_eq!(read["total_cents"], "9007199254740993");
    assert_eq!(read["legacy_edit_admitted"], true);
    let detail = doc(
        &mut client,
        serde_json::json!({"op":"item","item_id":"it_plan"}),
        13,
    )
    .result;
    assert_eq!(
        detail["plan"]["sections"][0]["lines"][0]["cost_cents"],
        "9007199254740993"
    );
    let toggle = doc(
        &mut client,
        serde_json::json!({"op":"toggle_plan_line","item_id":"it_plan","line_id":"ln_exact","observed_hash":read["content_hash"]}),
        14,
    );
    assert_eq!(
        toggle.result["item"]["plan"]["sections"][0]["lines"][0]["done"],
        true
    );
    assert!(client.document_request(&serde_json::json!({"op":"toggle_plan_line","item_id":"it_plan","line_id":"ln_exact","observed_hash":read["content_hash"]}).to_string(),15).is_err());
    let undo = doc(
        &mut client,
        serde_json::json!({"op":"undo","token":toggle.result["undo"]}),
        16,
    );
    assert_eq!(
        undo.result["plan"]["sections"][0]["lines"][0]["done"],
        false
    );
    assert!(client
        .document_request(
            &serde_json::json!({"op":"undo","token":toggle.result["undo"]}).to_string(),
            17
        )
        .is_err());
    Ok(())
}

pub(super) fn browser_app_install_uses_request_locale_and_preserves_signed_history(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, mut log) = document_fixture()?;
    let historical = clear(
        &mut log,
        &author,
        EventBody::MessagePosted {
            message: "msg_historical_app_copy".into(),
            text: "Mini-app added to this Space.".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
            forwarded_author: None,
        },
        4,
    )?;
    let original = log.events().last().unwrap().clone();
    let original_bytes = serde_json::to_vec(&original).unwrap();
    client.receive(&historical, 5).unwrap();

    for (locale, expected, now) in [
        ("en-US", "App added to this Space.", 10),
        ("pt-BR", "App adicionado a este Espaço.", 20),
    ] {
        let installed = client
            .app_request(
                &serde_json::json!({
                    "op": "install_app", "space_id": log.space(), "app_id": "list",
                    "args_json": serde_json::json!({"title": locale}).to_string(),
                })
                .to_string(),
                now,
                locale,
            )
            .unwrap();
        let item = installed.result["id"].as_str().unwrap().to_owned();
        let update = installed.update.unwrap();
        let published = update
            .frames
            .iter()
            .filter_map(|frame| match ClientFrame::decode(frame).unwrap() {
                ClientFrame::Publish { env } => env.plain_event(),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(published.len(), 3);
        assert!(matches!(published[0].body, EventBody::ItemCreated { .. }));
        assert!(matches!(published[1].body, EventBody::GrantIssued { .. }));
        assert!(matches!(&published[2].body,
            EventBody::MessagePosted { text, attaches: Some(attached), .. }
                if text == expected && attached == &item));
        confirm_documents(&mut client, &mut log, update, now + 1)?;
        let snapshot = client.update(Vec::new()).unwrap().snapshot;
        client = Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
        client.bootstrap().unwrap();
        client
            .receive(&ServerFrame::SyncDone.encode(), now + 2)
            .unwrap();
        let preserved = client.logs[log.space()]
            .events()
            .iter()
            .find(|event| event.hash == original.hash)
            .unwrap();
        assert_eq!(serde_json::to_vec(preserved).unwrap(), original_bytes);
        let conversation = client.conversations().remove(0);
        assert!(conversation
            .messages
            .iter()
            .any(|message| message.text == expected
                && message
                    .card
                    .as_ref()
                    .is_some_and(|card| card["item_id"] == item)));
        assert!(conversation
            .messages
            .iter()
            .any(|message| message.text == "Mini-app added to this Space."));
    }
    Ok(())
}

pub(super) fn browser_miniapps_use_shared_rules_item_scoped_grants_and_confirmations(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, mut log) = document_fixture()?;
    let made=client.app_request(&serde_json::json!({"op":"install_app","space_id":log.space(),"app_id":"pet","args_json":"{\"name\":\"Jumento\"}"}).to_string(),10,"pt-BR").unwrap();
    let item = made.result["id"].as_str().unwrap().to_owned();
    assert_eq!(made.result["app"]["trust"], "Act");
    let update = made.update.unwrap();
    let published = update
        .frames
        .iter()
        .filter_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Publish { env } => env.plain_event(),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(published.len(), 3);
    assert!(matches!(published[0].body, EventBody::ItemCreated { .. }));
    assert!(matches!(published[1].body, EventBody::GrantIssued { .. }));
    assert!(matches!(published[2].body, EventBody::MessagePosted { .. }));
    let timeline = client.conversations();
    let card = timeline[0]
        .messages
        .iter()
        .find_map(|message| message.card.as_ref())
        .unwrap();
    assert_eq!(card["item_id"], item);
    assert_eq!(card["app"]["app_id"], "pet");
    confirm_documents(&mut client, &mut log, update, 11)?;
    let read=client.app_request(&serde_json::json!({"op":"app_call_tool","item_id":item,"tool":"pet_get_state","args_json":"{}","confirmed":false}).to_string(),12,"pt-BR").unwrap();
    assert_eq!(read.result["status"], "Done");
    assert!(read.update.is_none());
    let fed=client.app_request(&serde_json::json!({"op":"app_call_tool","item_id":item,"tool":"pet_feed","args_json":"{}","confirmed":false}).to_string(),13,"pt-BR").unwrap();
    assert_eq!(fed.result["status"], "Done");
    assert_eq!(fed.result["item"]["version"], 2);
    let count = client.saved.outbox.len();
    let needs=client.app_request(&serde_json::json!({"op":"app_call_tool","item_id":item,"tool":"pet_release","args_json":"{}","confirmed":false}).to_string(),14,"pt-BR").unwrap();
    assert_eq!(needs.result["status"], "NeedsConfirmation");
    assert!(needs.update.is_none());
    assert_eq!(client.saved.outbox.len(), count);
    let released=client.app_request(&serde_json::json!({"op":"app_call_tool","item_id":item,"tool":"pet_release","args_json":"{}","confirmed":true}).to_string(),15,"pt-BR").unwrap();
    assert_eq!(released.result["status"], "Done");
    assert_eq!(released.result["item"]["version"], 3);
    let resource = client
        .app_request(
            &serde_json::json!({"op":"app_resource","uri":"ui://builtin/pet"}).to_string(),
            16,
            "pt-BR",
        )
        .unwrap();
    assert_eq!(
        resource.result["text"],
        miniapps::spec("pet").unwrap().html.unwrap()
    );
    let specs = client
        .app_request(r#"{"op":"app_specs"}"#, 17, "pt-BR")
        .unwrap();
    assert_eq!(
        specs.result.as_array().unwrap().len(),
        miniapps::specs().len()
    );
    Ok(())
}

pub(super) fn browser_task_checkbox_draft_survives_reread_and_cold_journal_restore(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, mut log) = document_fixture()?;
    let made = doc(
        &mut client,
        serde_json::json!({"op":"page_import_markdown","space_id":log.space(),"path":"checkbox.md","markdown":"# Checkbox\n\nOriginal café\n\n- [ ] Revisar documento\n"}),
        10,
    );
    let id = made.result["id"].as_str().unwrap().to_owned();
    confirm_documents(&mut client, &mut log, made.update.unwrap(), 11)?;
    let read = doc(
        &mut client,
        serde_json::json!({"op":"page","item_id":id}),
        12,
    )
    .result;
    assert_eq!(read["unsaved"], false);
    let published_markdown = doc(
        &mut client,
        serde_json::json!({"op":"page_markdown","item_id":id}),
        12,
    )
    .result;
    let mut task = read["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|block| block["kind"] == "task")
        .unwrap()
        .clone();
    assert_eq!(task["checked"], false);
    task["checked"] = true.into();
    let order = read["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["id"].clone())
        .collect::<Vec<_>>();
    let applied = doc(
        &mut client,
        serde_json::json!({"op":"page_apply","item_id":id,"mutation_id":"check-task","edit_context":read["edit_context"],"order":order,"changed":[task]}),
        13,
    );
    let assert_checked_draft = |value: &serde_json::Value| {
        assert_eq!(
            value["unsaved"], true,
            "accepted toggle must create a visible local draft"
        );
        let task = value["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|block| block["kind"] == "task")
            .unwrap();
        assert_eq!(task["checked"], true, "accepted toggle must remain checked");
    };
    assert_checked_draft(&applied.result["page"]);
    let snapshot = applied.update.unwrap().snapshot;
    let read_again = doc(
        &mut client,
        serde_json::json!({"op":"page","item_id":id}),
        14,
    )
    .result;
    assert_checked_draft(&read_again);
    let exported = doc(
        &mut client,
        serde_json::json!({"op":"page_markdown","item_id":id}),
        15,
    )
    .result;
    assert!(exported.as_str().unwrap().contains("[x] Revisar documento"));
    let mut reopened =
        Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    let restored = doc(
        &mut reopened,
        serde_json::json!({"op":"page","item_id":id}),
        16,
    )
    .result;
    assert_checked_draft(&restored);
    let exported = doc(
        &mut reopened,
        serde_json::json!({"op":"page_markdown","item_id":id}),
        17,
    )
    .result;
    assert!(exported.as_str().unwrap().contains("[x] Revisar documento"));
    let mut unchecked = restored["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|block| block["kind"] == "task")
        .unwrap()
        .clone();
    unchecked["checked"] = false.into();
    let restored_order = restored["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| block["id"].clone())
        .collect::<Vec<_>>();
    let reverted = doc(
        &mut reopened,
        serde_json::json!({"op":"page_apply","item_id":id,"mutation_id":"uncheck-task","edit_context":restored["edit_context"],"order":restored_order,"changed":[unchecked]}),
        18,
    );
    let assert_clean = |value: &serde_json::Value| {
        assert_eq!(
            value["unsaved"], false,
            "returning to published content clears the draft difference"
        );
        assert_eq!(
            value["blocks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|block| block["kind"] == "task")
                .unwrap()["checked"],
            false
        );
    };
    assert_clean(&reverted.result["page"]);
    let clean_snapshot = reverted.update.unwrap().snapshot;
    assert_clean(
        &doc(
            &mut reopened,
            serde_json::json!({"op":"page","item_id":id}),
            19,
        )
        .result,
    );
    assert_eq!(
        doc(
            &mut reopened,
            serde_json::json!({"op":"page_markdown","item_id":id}),
            20
        )
        .result,
        published_markdown
    );
    let mut clean_reopened = Client::new(
        reopened.account.clone(),
        reopened.signer.clone(),
        &clean_snapshot,
    )
    .unwrap();
    assert_clean(
        &doc(
            &mut clean_reopened,
            serde_json::json!({"op":"page","item_id":id}),
            21,
        )
        .result,
    );
    assert_eq!(
        doc(
            &mut clean_reopened,
            serde_json::json!({"op":"page_markdown","item_id":id}),
            22
        )
        .result,
        published_markdown
    );
    Ok(())
}

pub(super) fn received_app_owned_documents_keep_manifest_history_without_becoming_file_cards(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, mut log) = document_fixture()?;
    let bytes = br#"{"type":"excalidraw","version":2,"elements":[],"appState":{},"files":{}}"#;
    let sealed = crate::media::seal_file(
        "drawing.excalidraw",
        "application/vnd.excalidraw+json",
        bytes,
        None,
    )
    .unwrap();
    let document = types::AppDoc {
        view: None,
        app: "excalidraw".into(),
        resource_uri: "ui://zoen/excalidraw.html".into(),
        title: "Drawing".into(),
        state_json: r#"{"title":"Drawing","elementCount":0,"textCount":0}"#.into(),
        document: Some(sealed.doc.clone()),
    };
    let created = clear(
        &mut log,
        &author,
        EventBody::ItemCreated {
            item: "it_drawing".into(),
            kind: types::ItemKind::App,
            content: types::ItemContent::App(document),
            origin: "Created".into(),
        },
        10,
    )?;
    client.receive(&created, 11).unwrap();
    let posted = clear(
        &mut log,
        &author,
        EventBody::MessagePosted {
            message: "msg_drawing".into(),
            text: "Drawing".into(),
            attaches: Some("it_drawing".into()),
            reply: None,
            media: None,
            forwarded: false,
            forwarded_author: None,
        },
        12,
    )?;
    client.receive(&posted, 13).unwrap();
    let response = doc(
        &mut client,
        serde_json::json!({"op":"file_manifest","item_id":"it_drawing","version":1}),
        14,
    );
    assert_eq!(
        response.result["doc"],
        serde_json::to_value(&sealed.doc).unwrap()
    );
    let conversation = client.conversations();
    let message = conversation[0].messages.last().unwrap();
    assert!(message.file.is_none());
    assert_eq!(
        message.card.as_ref().unwrap()["app"]["app_id"],
        "excalidraw"
    );
    let snapshot = client.update(Vec::new()).unwrap().snapshot;
    let mut restored =
        Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    let history = doc(
        &mut restored,
        serde_json::json!({"op":"file_manifest","item_id":"it_drawing","version":1}),
        15,
    );
    assert_eq!(history.result["doc"], response.result["doc"]);
    let installed=restored.app_request(&serde_json::json!({"op":"install_app","space_id":log.space(),"app_id":"excalidraw","args_json":"{}"}).to_string(),16,"en").unwrap();
    assert_eq!(installed.result["kind_id"], "app");
    let before = restored.saved.outbox.len();
    assert!(restored.app_request(&serde_json::json!({"op":"app_call_tool","item_id":"it_drawing","tool":"app_document_read","args_json":"{}","confirmed":false}).to_string(),17,"en").is_err());
    assert_eq!(restored.saved.outbox.len(), before);
    Ok(())
}
