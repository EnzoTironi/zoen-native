//! Whole collaborative edits cases on the same original parent fixture.

use super::*;

pub(super) fn two_editors_preserve_unseen_text_and_insertions_while_deleting_and_moving_blocks() {
    let (engine, first) = fixture();
    let second = engine.page(first.item_id.clone()).unwrap();
    assert_ne!(first.edit_context, second.edit_context);
    let mut foreign = first.blocks.clone();
    foreign[3].text = "Gamma from the other editor".into();
    let mut inserted = foreign[2].clone();
    inserted.id = "foreign-block".into();
    inserted.text = "New remote block".into();
    foreign.insert(2, inserted);
    apply(&engine, &first, "first-edit", &foreign).unwrap();
    let mut local = second.blocks.clone();
    local.remove(2); // Delete Beta, which this editor observed.
    let moved = local.pop().unwrap();
    local.insert(0, moved);
    local.iter_mut().find(|b| b.text == "Alpha").unwrap().text = "Alpha from this editor".into();
    let result = apply(&engine, &second, "second-edit", &local).unwrap();
    assert!(result
        .page
        .blocks
        .iter()
        .any(|b| b.text == "New remote block"));
    assert!(result
        .page
        .blocks
        .iter()
        .any(|b| b.text == "Gamma from the other editor"));
    assert!(result
        .page
        .blocks
        .iter()
        .any(|b| b.text == "Alpha from this editor"));
    assert!(!result
        .page
        .blocks
        .iter()
        .any(|b| b.id == second.blocks[2].id));
    assert_eq!(result.page.blocks[0].id, second.blocks[3].id);
    assert!(result.page.unsaved);
}

pub(super) fn lost_reply_continuation_can_undo_and_redo_without_duplicate_text() {
    let (engine, observed) = fixture();
    let mut edited = observed.blocks.clone();
    edited[1].text.push_str(" typed once");
    let first = apply(&engine, &observed, "typing", &edited).unwrap();
    engine.lock().reload().unwrap();
    let undone = apply(&engine, &observed, "undo", &observed.blocks).unwrap();
    assert_eq!(undone.page.blocks, observed.blocks);
    assert!(!undone.page.unsaved);
    let redone = apply(&engine, &observed, "redo", &edited).unwrap();
    assert_eq!(redone.page.blocks, edited);
    assert_eq!(redone.applied_content_hash, first.applied_content_hash);
    let retry = apply(&engine, &observed, "typing", &edited).unwrap();
    assert!(retry.replayed);
    assert_eq!(retry.applied_content_hash, first.applied_content_hash);
    assert_eq!(retry.page.blocks, edited);
    assert_eq!(retry.mutation_id, "typing");
    let mut different = edited.clone();
    different[1].text.push('!');
    assert!(matches!(
        apply(&engine, &observed, "typing", &different),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(
        engine.page(observed.item_id.clone()).unwrap().blocks,
        edited
    );
}

pub(super) fn lost_reply_continuation_can_remove_its_own_new_block() {
    let (engine, observed) = fixture();
    let mut edited = observed.blocks.clone();
    let mut new = edited[1].clone();
    new.id = "temporary-block".into();
    new.text = "A block the user then deletes".into();
    edited.push(new);
    apply(&engine, &observed, "insert", &edited).unwrap();
    engine.lock().reload().unwrap();
    let removed = apply(&engine, &observed, "delete", &observed.blocks).unwrap();
    assert_eq!(removed.page.blocks, observed.blocks);
}

pub(super) fn conflicting_external_ids_are_refused_before_persisting_merged_content() {
    let (engine, first) = fixture();
    let second = engine.page(first.item_id.clone()).unwrap();
    let mut a = first.blocks.clone();
    let mut block = a[1].clone();
    block.id = "same-external-id".into();
    block.text = "First".into();
    a.push(block.clone());
    apply(&engine, &first, "first-insert", &a).unwrap();
    let rows = journal_rows(&engine);
    let mut b = second.blocks.clone();
    block.text = "Second".into();
    b.push(block);
    assert!(apply(&engine, &second, "second-insert", &b).is_err());
    assert_eq!(journal_rows(&engine), rows);
    let page = engine.page(first.item_id.clone()).unwrap();
    assert!(page.can_edit);
    assert_eq!(page.blocks, a);
}

pub(super) fn invalid_formatting_is_atomic_even_with_a_delete_and_reorder() {
    let (engine, observed) = fixture();
    let mut edited = observed.blocks.clone();
    edited.remove(2);
    edited.reverse();
    edited[0].spans.push(TextSpanDto {
        start: 1,
        end: 999,
        key: "b".into(),
        value: String::new(),
    });
    assert!(apply(&engine, &observed, "invalid", &edited).is_err());
    assert!(journal_rows(&engine).is_empty());
    assert_eq!(
        engine.page(observed.item_id.clone()).unwrap().blocks,
        observed.blocks
    );
}
