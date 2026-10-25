//! Original whole test bodies, called by their unchanged registered facade.
use super::*;

pub(super) fn browser_new_document_notes_follow_language_without_rewriting_received_history(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for (language, created, imported, untitled, page_label, restore_note) in [
        (
            localization::Lang::En,
            "Created",
            "Imported",
            "Untitled",
            "Page",
            "Restored v1",
        ),
        (
            localization::Lang::Pt,
            "Criado",
            "Importado",
            "Sem título",
            "Página",
            "Restaurou a v1",
        ),
    ] {
        let _language = localization::enter(language);
        let (mut client, author, mut log) = document_fixture()?;
        let made = doc(
            &mut client,
            serde_json::json!({"op":"page_create","space_id":log.space(),"title":""}),
            10,
        );
        assert_eq!(made.result["origin"], created);
        assert_eq!(made.result["title"], untitled);
        assert_eq!(made.result["kind_id"], "page");
        assert_eq!(made.result["kind_label"], page_label);
        confirm_documents(&mut client, &mut log, made.update.unwrap(), 11)?;
        let made = doc(
            &mut client,
            serde_json::json!({"op":"page_import_markdown","space_id":log.space(),"path":"notas/original.md","markdown":"# Título em português 🪴\n"}),
            12,
        );
        assert_eq!(made.result["origin"], imported);
        assert_eq!(made.result["title"], "Título em português 🪴");
        assert_eq!(made.result["path"], "notas/original.md");
        confirm_documents(&mut client, &mut log, made.update.unwrap(), 13)?;
        let received = clear(
            &mut log,
            &author,
            EventBody::ItemCreated {
                item: "it_literal_history".into(),
                kind: types::ItemKind::Note,
                content: types::ItemContent::Text {
                    text: "Texto original em português".into(),
                },
                origin: "Criado no iOS — manter literal".into(),
            },
            14,
        )?;
        client.receive(&received, 15).unwrap();
        let received = clear(
            &mut log,
            &author,
            EventBody::ItemVersioned {
                item: "it_literal_history".into(),
                content: types::ItemContent::Text {
                    text: "Outra versão em português".into(),
                },
                note: "Editou no iOS — manter literal".into(),
            },
            16,
        )?;
        client.receive(&received, 17).unwrap();
        let restored = doc(
            &mut client,
            serde_json::json!({"op":"restore_version","item_id":"it_literal_history","version":1}),
            18,
        );
        assert_eq!(restored.result["versions"][0]["note"], restore_note);
        assert_eq!(
            restored.result["versions"][1]["note"],
            "Editou no iOS — manter literal"
        );
        assert_eq!(
            restored.result["versions"][2]["note"],
            "Criado no iOS — manter literal"
        );
        assert_eq!(restored.result["text"], "Texto original em português");
    }
    Ok(())
}

pub(super) fn browser_pages_share_native_codec_persist_drafts_and_replay_receipts(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, mut log) = document_fixture()?;
    let made = doc(
        &mut client,
        serde_json::json!({"op":"page_import_markdown","space_id":log.space(),"path":"notes/viagem.md","markdown":"# Viagem 🪴\n\nTexto **marcado**.\n"}),
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
    assert_eq!(read["path"], "notes/viagem.md");
    let mut changed = read["blocks"][1].clone();
    changed["text"] = "Texto **preservado** 🪴".into();
    changed["spans"] = serde_json::json!([]);
    let request = serde_json::json!({"op":"page_apply","item_id":id,"mutation_id":"first","edit_context":read["edit_context"],"order":read["blocks"].as_array().unwrap().iter().map(|b|b["id"].clone()).collect::<Vec<_>>(),"changed":[changed]});
    let applied = doc(&mut client, request.clone(), 13);
    assert_eq!(applied.result["replayed"], false);
    assert_eq!(applied.result["page"]["unsaved"], true);
    let snapshot = applied.update.unwrap().snapshot;
    let mut reopened =
        Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    let replay = doc(&mut reopened, request.clone(), 14);
    assert_eq!(replay.result["replayed"], true);
    assert_eq!(
        replay.result["applied_content_hash"],
        applied.result["applied_content_hash"]
    );
    let exported = doc(
        &mut reopened,
        serde_json::json!({"op":"page_markdown","item_id":id}),
        15,
    );
    assert!(exported.result.as_str().unwrap().contains("preservado"));
    let mut wrong = request;
    wrong["changed"][0]["text"] = "Outra edição".into();
    assert!(reopened.document_request(&wrong.to_string(), 16).is_err());
    let committed = doc(
        &mut reopened,
        serde_json::json!({"op":"page_commit","item_id":id,"note":"Editou no navegador"}),
        17,
    );
    assert_eq!(committed.result, true);
    let queued_page = doc(
        &mut reopened,
        serde_json::json!({"op":"page","item_id":id}),
        18,
    )
    .result;
    assert_eq!(queued_page["unsaved"], false);
    assert_eq!(queued_page["pending_sync"], true);
    Ok(())
}

pub(super) fn browser_stale_page_editor_merges_unseen_blocks_and_restores_as_a_new_change(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use base64::Engine as _;
    let (mut client, author, mut log) = document_fixture()?;
    let page = ::documents::Page::from_markdown("# Original\n\nTexto antigo\n").unwrap();
    let content = types::PageContent {
        title: "Original".into(),
        path: String::new(),
        loro: base64::engine::general_purpose::STANDARD.encode(page.snapshot()),
        blob: None,
        manifest: None,
    };
    let create = clear(
        &mut log,
        &author,
        EventBody::ItemCreated {
            item: "it_merge".into(),
            kind: types::ItemKind::Page,
            content: types::ItemContent::Page(content),
            origin: "Criado no nativo".into(),
        },
        10,
    )?;
    client.receive(&create, 11).unwrap();
    let read = doc(
        &mut client,
        serde_json::json!({"op":"page","item_id":"it_merge"}),
        12,
    )
    .result;
    let before = page.version();
    let mut order = page
        .blocks()
        .iter()
        .map(|b| b.id.clone())
        .collect::<Vec<_>>();
    order.push("remote-block".into());
    page.apply(
        &order,
        &[::documents::BlockEdit {
            id: "remote-block".into(),
            kind: ::documents::Kind::Paragraph,
            text: "Mudança remota".into(),
            spans: vec![],
        }],
    )
    .unwrap();
    let version = clear(
        &mut log,
        &author,
        EventBody::ItemVersioned {
            item: "it_merge".into(),
            content: types::ItemContent::Page(types::PageContent {
                title: "Original".into(),
                path: String::new(),
                loro: base64::engine::general_purpose::STANDARD.encode(page.updates_since(&before)),
                blob: None,
                manifest: None,
            }),
            note: "Mudança concorrente".into(),
        },
        13,
    )?;
    client.receive(&version, 14).unwrap();
    let mut heading = read["blocks"][0].clone();
    heading["text"] = "Editado no navegador".into();
    let request = serde_json::json!({"op":"page_apply","item_id":"it_merge","mutation_id":"merge","edit_context":read["edit_context"],"order":read["blocks"].as_array().unwrap().iter().map(|b|b["id"].clone()).collect::<Vec<_>>(),"changed":[heading]});
    let merged = doc(&mut client, request, 15).result;
    assert_eq!(merged["page"]["blocks"].as_array().unwrap().len(), 3);
    assert!(merged["page"]["blocks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["text"] == "Mudança remota"));
    let committed = doc(
        &mut client,
        serde_json::json!({"op":"page_commit","item_id":"it_merge","note":"Mesclou"}),
        16,
    );
    confirm_documents(&mut client, &mut log, committed.update.unwrap(), 17)?;
    let past = doc(
        &mut client,
        serde_json::json!({"op":"page_at","item_id":"it_merge","version":1}),
        18,
    )
    .result;
    assert_eq!(past["blocks"].as_array().unwrap().len(), 2);
    assert_eq!(past["can_edit"], false);
    let restored = doc(
        &mut client,
        serde_json::json!({"op":"restore_version","item_id":"it_merge","version":1}),
        19,
    );
    assert_eq!(restored.result["version"], 4);
    let current = doc(
        &mut client,
        serde_json::json!({"op":"page","item_id":"it_merge"}),
        20,
    )
    .result;
    assert_eq!(current["blocks"].as_array().unwrap().len(), 2);
    assert_eq!(current["blocks"][0]["text"], "Original");
    Ok(())
}

pub(super) fn browser_large_page_upload_barrier_survives_reload_and_missing_download_waits(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use base64::Engine as _;
    let (mut client, author, mut log) = document_fixture()?;
    let original_snapshot = client.update(Vec::new()).unwrap().snapshot;
    let markdown = (0..8000)
        .map(|i| format!("Parágrafo {i}: conteúdo de uma página compartilhada 🪴.\n\n"))
        .collect::<String>();
    let made = doc(
        &mut client,
        serde_json::json!({"op":"page_import_markdown","space_id":log.space(),"path":"large.md","markdown":markdown}),
        10,
    );
    assert!(!made.uploads.is_empty());
    let update = made.update.unwrap();
    assert!(!update
        .frames
        .iter()
        .any(|frame| matches!(ClientFrame::decode(frame), Ok(ClientFrame::Publish { .. }))));
    let mut reopened = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &update.snapshot,
    )
    .unwrap();
    let resumed: Vec<String> = serde_json::from_str(&reopened.document_uploads().unwrap()).unwrap();
    assert_eq!(resumed, made.uploads);
    let queued = reopened
        .saved
        .outbox
        .values()
        .find_map(|p| p.inner.clone().or_else(|| p.envelope().ok()?.plain_event()))
        .unwrap();
    let types::EventBody::ItemCreated {
        item,
        content: types::ItemContent::Page(content),
        ..
    } = queued.body
    else {
        panic!("page creation")
    };
    let reference = content.blob.unwrap();
    let sealed = reopened.local_chunk(&reference.blob);
    let bytes = crate::media::open_chunk(&reference, &sealed).unwrap();
    let page = ::documents::Page::new();
    page.import(&bytes).unwrap();
    assert!(page.to_markdown().contains("Parágrafo 7999"));
    reopened.synced = true;
    let released = reopened.document_uploaded(&resumed).unwrap();
    assert!(released
        .frames
        .iter()
        .any(|frame| matches!(ClientFrame::decode(frame), Ok(ClientFrame::Publish { .. }))));
    let received = clear(
        &mut log,
        &author,
        EventBody::ItemCreated {
            item: item.clone(),
            kind: types::ItemKind::Page,
            content: types::ItemContent::Page(types::PageContent {
                title: "Grande".into(),
                path: "large.md".into(),
                loro: String::new(),
                blob: Some(reference.clone()),
                manifest: None,
            }),
            origin: "Outro cliente".into(),
        },
        20,
    )?;
    let mut remote = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &original_snapshot,
    )
    .unwrap();
    remote.receive(&received, 21).unwrap();
    let waiting = doc(
        &mut remote,
        serde_json::json!({"op":"page","item_id":item}),
        22,
    );
    assert_eq!(waiting.result["ready"], false);
    assert_eq!(waiting.result["can_edit"], false);
    assert_eq!(waiting.downloads.len(), 1);
    assert!(remote
        .document_cache_blob(reference.clone(), b"wrong".to_vec())
        .is_err());
    remote.document_cache_blob(reference, bytes).unwrap();
    let ready = doc(
        &mut remote,
        serde_json::json!({"op":"page_markdown","item_id":item}),
        23,
    );
    assert!(ready.result.as_str().unwrap().contains("Parágrafo 7999"));
    // The codec itself is the same binary snapshot native imports.
    assert!(
        base64::engine::general_purpose::STANDARD
            .encode(page.snapshot())
            .len()
            > 48 << 10
    );
    Ok(())
}
