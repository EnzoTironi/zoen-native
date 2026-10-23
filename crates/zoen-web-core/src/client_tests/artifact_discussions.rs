use super::*;
use serde_json::json;

fn read(client: &mut Client<HostSigner>, space: &str, item: &str) -> DocumentResponse {
    let before = serde_json::to_string(&client.saved).unwrap();
    let response = doc(
        client,
        json!({"op":"item_discussion","space_id":space,"item_id":item}),
        90,
    );
    assert!(response.update.is_none());
    assert!(response.uploads.is_empty() && response.downloads.is_empty());
    assert_eq!(
        serde_json::to_string(&client.saved).unwrap(),
        before,
        "reading cannot publish a draft or another post"
    );
    response
}

#[test]
fn browser_page_creation_is_one_stable_card_and_discussion_survives_ack_reply_and_reload() {
    let (mut client, author, mut log) = document_fixture();
    let space = log.space().to_owned();
    let created = doc(
        &mut client,
        json!({"op":"page_import_markdown","space_id":space,"path":"shared.md","markdown":"# Shared\n\nOriginal text\n"}),
        10,
    );
    let item = created.result["id"].as_str().unwrap().to_owned();
    let pending = read(&mut client, &space, &item).result;
    let root = pending["root_id"].as_str().unwrap().to_owned();
    let view = client.conversations();
    assert_eq!(view[0].messages.len(), 1);
    assert!(view[0].messages[0].text.is_empty());
    assert_eq!(view[0].messages[0].card.as_ref().unwrap()["item_id"], item);
    confirm_documents(&mut client, &mut log, created.update.unwrap(), 11);
    assert_eq!(read(&mut client, &space, &item).result, pending);
    let sent = client
        .queue_post(
            &space,
            "A real artifact comment",
            Some((root.clone(), true)),
            false,
            12,
        )
        .unwrap();
    confirm_documents(&mut client, &mut log, sent, 13);
    let original_content = client
        .document_item(&item)
        .unwrap()
        .current()
        .content
        .clone();
    let version = clear(
        &mut log,
        &author,
        EventBody::ItemVersioned {
            item: item.clone(),
            content: original_content,
            note: "New version, same discussion".into(),
        },
        14,
    );
    client.receive(&version, 15).unwrap();
    let snapshot = client.update(Vec::new()).unwrap().snapshot;
    let mut reopened =
        Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    assert_eq!(read(&mut reopened, &space, &item).result, pending);
    let view = reopened.conversations();
    let roots = view[0]
        .messages
        .iter()
        .filter(|message| message.id == root)
        .collect::<Vec<_>>();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].replies, 1);
    assert_eq!(roots[0].card.as_ref().unwrap()["version"], 2);
    assert_eq!(
        view[0]
            .messages
            .iter()
            .filter(|message| message.thread.is_some())
            .count(),
        1
    );
}

#[test]
fn browser_discussion_rejects_foreign_space_removed_membership_and_failed_local_creation() {
    let (mut client, author, mut log) = document_fixture();
    let space = log.space().to_owned();
    let created = doc(
        &mut client,
        json!({"op":"page_create","space_id":space,"title":"Private scope"}),
        10,
    );
    let item = created.result["id"].as_str().unwrap().to_owned();
    let request =
        |space: &str| json!({"op":"item_discussion","space_id":space,"item_id":item}).to_string();
    assert!(client
        .document_request(&request("foreign-space"), 11)
        .is_err());
    client
        .saved
        .outbox
        .values_mut()
        .for_each(|pending| pending.failure = Some("rejected".into()));
    assert!(client.document_request(&request(&space), 12).is_err());
    assert!(
        client.conversations()[0].messages.is_empty(),
        "failed creation is not a published card"
    );
    client
        .saved
        .outbox
        .values_mut()
        .for_each(|pending| pending.failure = None);
    confirm_documents(&mut client, &mut log, created.update.unwrap(), 13);
    let removed = clear(
        &mut log,
        &author,
        EventBody::MemberRemoved {
            identity: client.account.identity.clone(),
        },
        14,
    );
    client.receive(&removed, 15).unwrap();
    assert!(client.document_request(&request(&space), 16).is_err());
}

#[test]
fn browser_discussion_does_not_publish_private_draft_or_choose_forwarded_inline_or_nested_posts() {
    let (mut client, author, mut log) = document_fixture();
    let space = log.space().to_owned();
    let created = doc(
        &mut client,
        json!({"op":"page_create","space_id":space,"title":"Shared"}),
        10,
    );
    let item = created.result["id"].as_str().unwrap().to_owned();
    confirm_documents(&mut client, &mut log, created.update.unwrap(), 11);
    let root = read(&mut client, &space, &item).result;
    let page = doc(&mut client, json!({"op":"page","item_id":item}), 12).result;
    let mut block = page["blocks"][0].clone();
    block["text"] = json!("Unpublished secret draft");
    doc(
        &mut client,
        json!({"op":"page_apply","item_id":item,"mutation_id":"private-draft","edit_context":page["edit_context"],"order":page["blocks"].as_array().unwrap().iter().map(|block|block["id"].clone()).collect::<Vec<_>>(),"changed":[block]}),
        13,
    );
    let duplicate_content = client.document_item(&item).unwrap().versions[0]
        .content
        .clone();
    let duplicate = clear(
        &mut log,
        &author,
        EventBody::ItemCreated {
            item: item.clone(),
            kind: types::ItemKind::Page,
            content: duplicate_content,
            origin: "Rejected duplicate creation".into(),
        },
        16,
    );
    client.receive(&duplicate, 17).unwrap();
    let duplicate_event = log.events().last().unwrap();
    let before = serde_json::to_string(&client.saved).unwrap();
    for alias in [&duplicate_event.client_id, &duplicate_event.hash] {
        assert!(client
            .queue_post(
                &space,
                "Cannot reply to a rejected creation",
                Some((alias.clone(), true)),
                false,
                18
            )
            .is_err());
        assert!(client.resolve_post(&space, alias).is_none());
    }
    assert_eq!(serde_json::to_string(&client.saved).unwrap(), before);
    for (index, reply, forwarded) in [
        (0, None, true),
        (
            1,
            Some(types::ReplyRef::inline(root["root_id"].as_str().unwrap())),
            false,
        ),
        (
            2,
            Some(types::ReplyRef::thread(root["root_id"].as_str().unwrap())),
            false,
        ),
    ] {
        let event = clear(
            &mut log,
            &author,
            EventBody::MessagePosted {
                message: format!("msg_other_{index}"),
                text: String::new(),
                attaches: Some(item.clone()),
                reply,
                media: None,
                forwarded,
            },
            20 + index,
        );
        client.receive(&event, 30 + index).unwrap();
    }
    let discussion = read(&mut client, &space, &item).result;
    assert_eq!(discussion, root);
    assert!(!discussion.to_string().contains("Unpublished secret draft"));
    let published = doc(
        &mut client,
        json!({"op":"page_at","item_id":item,"version":1}),
        40,
    )
    .result;
    assert!(!published.to_string().contains("Unpublished secret draft"));
    assert_eq!(client.document_item(&item).unwrap().versions.len(), 1);
}
