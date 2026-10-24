use super::*;
use crate::{Client, EntryKind};
use std::sync::Arc;

fn fixture(path: &str) -> (Arc<Client>, String) {
    let client = Client::open(path.into(), "en".into()).unwrap();
    client.seed_demo_if_empty().unwrap();
    let space = client
        .spaces()
        .into_iter()
        .find(|s| s.title == "Paraty with Marina")
        .unwrap()
        .id;
    (client, space)
}

#[test]
fn page_creation_is_one_original_card_and_discussion_reads_never_append_events() {
    let (client, space) = fixture(":memory:");
    let before = client.lock().logs[&space].events().len();
    let item = client
        .page_create(space.clone(), "A shared idea".into())
        .unwrap();
    let engine = client.lock();
    let root = engine
        .item_discussion(&space, &item.id)
        .unwrap()
        .root
        .unwrap();
    assert_eq!(engine.logs[&space].events().len(), before + 1);
    assert_eq!(
        root.id,
        engine.logs[&space].events().last().unwrap().client_id
    );
    assert_eq!(root.thread_replies, 0);
    assert!(
        matches!(&root.kind, EntryKind::Message { text, card: Some(card) }
        if text.is_empty() && card.item_id == item.id && card.kind_id == "page")
    );
    for _ in 0..3 {
        assert_eq!(
            engine
                .item_discussion(&space, &item.id)
                .unwrap()
                .root
                .unwrap()
                .id,
            root.id
        );
    }
    assert_eq!(engine.logs[&space].events().len(), before + 1);
    assert_eq!(
        engine
            .timeline(&space)
            .unwrap()
            .iter()
            .filter(|e| e.id == root.id)
            .count(),
        1
    );
}

#[test]
fn imported_page_thread_and_edited_page_recover_the_same_original_root() {
    let directory = std::env::temp_dir().join(types::new_id("discussion-test").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory
        .join("client.sqlite")
        .to_string_lossy()
        .into_owned();
    let (item, space, root, reply) = {
        let (client, space) = fixture(&path);
        let item = client
            .page_import_markdown(
                space.clone(),
                "ideas/page.md".into(),
                "# Idea\n\nOriginal".into(),
            )
            .unwrap()
            .id;
        let root = client
            .lock()
            .item_discussion(&space, &item)
            .unwrap()
            .root
            .unwrap()
            .id;
        let reply = client
            .send_reply(
                space.clone(),
                "Review this together".into(),
                root.clone(),
                true,
            )
            .unwrap()
            .id;
        let page = client.page(item.clone()).unwrap();
        let mut block = page.blocks[1].clone();
        block.text = "Revised together".into();
        client
            .page_apply_from(
                item.clone(),
                "edit-one".into(),
                page.edit_context,
                page.blocks.iter().map(|b| b.id.clone()).collect(),
                vec![block],
            )
            .unwrap();
        client.page_commit(item.clone(), "Reviewed".into()).unwrap();
        assert_eq!(
            client
                .lock()
                .item_discussion(&space, &item)
                .unwrap()
                .root
                .unwrap()
                .id,
            root
        );
        (item, space, root, reply)
    };
    {
        let client = Client::open(path, "en".into()).unwrap();
        let result = client.lock().item_discussion(&space, &item).unwrap();
        assert_eq!(result.item_id, item);
        assert_eq!(result.space_id, space);
        let recovered = result.root.unwrap();
        assert_eq!(recovered.id, root);
        assert_eq!(recovered.thread_replies, 1);
        let thread = client.thread(space.clone(), root.clone()).unwrap();
        assert_eq!(thread.len(), 2);
        assert_eq!(thread[1].id, reply);
        assert_eq!(thread[1].in_thread.as_deref(), Some(root.as_str()));
        assert_eq!(
            client
                .timeline(space.clone())
                .unwrap()
                .iter()
                .filter(|e| e.id == root)
                .count(),
            1
        );
        assert!(client
            .page_markdown(item)
            .unwrap()
            .contains("Revised together"));
        assert!(client.verify_log(space).valid);
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn discussion_refuses_a_substituted_space_or_missing_current_membership() {
    let (client, space) = fixture(":memory:");
    let item = client
        .page_create(space.clone(), "Scoped idea".into())
        .unwrap();
    let other = client
        .spaces()
        .into_iter()
        .find(|s| s.id != space)
        .unwrap()
        .id;
    let me = client.me().unwrap().id;
    let mut engine = client.lock();
    assert!(engine.item_discussion(&other, &item.id).is_err());
    assert!(engine.item_discussion(&space, "missing-item").is_err());
    let count = engine.logs[&space].events().len();
    engine
        .state
        .spaces
        .get_mut(&space)
        .unwrap()
        .members
        .retain(|(id, _)| id != &me);
    assert!(matches!(
        engine.item_discussion(&space, &item.id),
        Err(CoreError::Forbidden { .. })
    ));
    assert_eq!(engine.logs[&space].events().len(), count);
}

#[test]
fn a_private_page_draft_is_not_published_by_reading_or_opening_its_thread() {
    let (client, space) = fixture(":memory:");
    let item = client
        .page_create(space.clone(), "Private draft".into())
        .unwrap();
    let page = client.page(item.id.clone()).unwrap();
    let mut block = page.blocks.last().unwrap().clone();
    block.text = "Unpublished private words".into();
    client
        .page_apply_from(
            item.id.clone(),
            "private-input".into(),
            page.edit_context,
            page.blocks.iter().map(|b| b.id.clone()).collect(),
            vec![block],
        )
        .unwrap();
    let before = client.lock().logs[&space].events().len();
    let root = client
        .lock()
        .item_discussion(&space, &item.id)
        .unwrap()
        .root
        .unwrap();
    assert_eq!(client.thread(space.clone(), root.id).unwrap().len(), 1);
    assert_eq!(client.lock().logs[&space].events().len(), before);
    assert_eq!(client.item(item.id.clone()).unwrap().version, 1);
    assert!(client.page(item.id).unwrap().unsaved);
}
