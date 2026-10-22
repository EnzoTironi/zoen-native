//! Whole admission cases on the same original parent fixture.

use super::*;

pub(super) fn readers_and_removed_members_cannot_change_cached_pages_or_journals() {
    let (engine, observed) = fixture();
    let me = engine.me().unwrap().id;
    let mut edited = observed.blocks.clone();
    edited[1].text = "Refused write".into();
    for role in [Some(Role::Reader), None] {
        {
            let mut e = engine.lock();
            let members = &mut e.state.spaces.get_mut(&observed.space_id).unwrap().members;
            members.retain(|(id, _)| id != &me);
            if let Some(role) = role {
                members.push((me.clone(), role));
            }
        }
        assert!(matches!(
            apply(&engine, &observed, "reader", &edited),
            Err(CoreError::Forbidden { .. })
        ));
        assert!(matches!(
            engine.page_commit(observed.item_id.clone(), "Save".into()),
            Err(CoreError::Forbidden { .. })
        ));
        let page = engine.page(observed.item_id.clone()).unwrap();
        assert!(!page.can_edit);
        assert!(page.edit_context.is_empty());
        assert_eq!(page.blocks, observed.blocks);
        assert!(journal_rows(&engine).is_empty());
    }
}

pub(super) fn shared_session_does_not_activate_public_plan_page_writers() {
    let (engine, observed) = fixture();
    let plan = engine
        .items()
        .into_iter()
        .find(|item| item.kind_id == "plan")
        .unwrap();
    let encrypted = journal_rows(&engine);
    let events = engine.lock().logs[&plan.space_id].events().len();
    assert!(engine.page(plan.id.clone()).is_err());
    assert!(engine.page_at(plan.id.clone(), 1).is_err());
    assert!(engine
        .page_commit(plan.id.clone(), "Unsupported Plan writer".into())
        .is_err());
    assert!(engine
        .page_apply_from(
            plan.id.clone(),
            "unsupported-plan".into(),
            observed.edit_context,
            Vec::new(),
            Vec::new()
        )
        .is_err());
    assert_eq!(engine.item(plan.id.clone()).unwrap().version, plan.version);
    assert_eq!(engine.lock().logs[&plan.space_id].events().len(), events);
    assert_eq!(journal_rows(&engine), encrypted);
}
