use super::*;

#[test]
fn explicit_install_does_not_raise_agent_trust_and_limits_the_app_grant_to_its_item() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina");
    let agent = zoen(&e);
    e.set_trust(agent.id.clone(), space.id.clone(), TrustLevelDto::Listen)
        .unwrap();
    let pet = e
        .install_app(
            space.id.clone(),
            "pet".into(),
            r#"{"name":"Android Burrico"}"#.into(),
        )
        .unwrap();
    assert_eq!(pet.created_by.id, e.me().unwrap().id);
    assert_eq!(pet.app.as_ref().unwrap().trust, TrustLevelDto::Act);
    let profile = e.agent_profile(agent.id);
    assert_eq!(
        profile
            .spaces
            .iter()
            .find(|s| s.space_id == space.id)
            .unwrap()
            .level,
        TrustLevelDto::Listen
    );
    let fed = e
        .app_call_tool(pet.id.clone(), "pet_feed".into(), "{}".into(), false)
        .unwrap();
    assert_eq!(fed.status, AppCallStatus::Done);
    assert_eq!(
        e.app_call_tool(pet.id, "pet_release".into(), "{}".into(), false)
            .unwrap()
            .status,
        AppCallStatus::NeedsConfirmation
    );
    assert!(e.verify_log(space.id).valid);
}

#[test]
fn explicit_install_rejects_invalid_inputs_before_creating_any_item() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina");
    let before = e.items().len();
    for (id, args) in [("unknown", "{}"), ("pet", "{"), ("pet", "[]")] {
        assert!(e
            .install_app(space.id.clone(), id.into(), args.into())
            .is_err());
    }
    assert!(e
        .install_app("missing-space".into(), "pet".into(), "{}".into())
        .is_err());
    assert_eq!(e.items().len(), before);
}

#[test]
fn member_roles_match_the_shared_membership_projection() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina");
    let roles = e.member_roles(space.id.clone()).unwrap();
    assert_eq!(roles.len(), space.members.len());
    assert!(roles
        .iter()
        .all(|r| space.members.iter().any(|m| m.id == r.identity_id)));
    assert!(roles
        .iter()
        .all(|r| ["owner", "admin", "member", "reader"].contains(&r.role.as_str())));
    assert!(roles
        .iter()
        .any(|r| r.identity_id == e.me().unwrap().id && r.role == "owner"));
    assert!(e.member_roles("missing".into()).is_err());
}

#[test]
fn rendered_file_versions_record_the_output_format_and_preserve_original_bytes() {
    let e = seeded();
    let space = find_space(&e, "Paraty com a Marina");
    let original = e
        .file_add(
            space.id.clone(),
            "".into(),
            "recording.mp3".into(),
            "audio/mpeg".into(),
            vec![1, 2, 3],
            None,
        )
        .unwrap();
    let encoded = e
        .file_new_version_typed(
            original.id.clone(),
            vec![4, 5],
            None,
            "Trimmed".into(),
            "recording.m4a".into(),
            "audio/mp4".into(),
        )
        .unwrap();
    assert_eq!(encoded.version, 2);
    assert_eq!(encoded.file.as_ref().unwrap().name, "recording.m4a");
    assert_eq!(encoded.file.as_ref().unwrap().mime, "audio/mp4");
    let preview = e.item_at(original.id.clone(), 1).unwrap();
    assert_eq!(preview.file.as_ref().unwrap().mime, "audio/mpeg");
    assert_eq!(preview.file.as_ref().unwrap().name, "recording.mp3");
    assert_eq!(preview.version, 1);
    assert_eq!(e.item(original.id.clone()).unwrap().version, 2);
    assert_eq!(
        e.file_bytes(original.id.clone(), Some(1)).unwrap(),
        Some(vec![1, 2, 3])
    );
    assert_eq!(
        e.file_bytes(original.id.clone(), Some(2)).unwrap(),
        Some(vec![4, 5])
    );
    assert!(e
        .file_new_version_typed(
            original.id,
            vec![6],
            None,
            "Invalid".into(),
            "../bad.mp4".into(),
            "video/mp4".into()
        )
        .is_err());
    assert!(e.verify_log(space.id).valid);
}

#[test]
fn version_previews_show_historical_plans_and_pages_without_restoring_them() {
    let e = seeded();
    let plan = paraty_plan(&e);
    let before = e.item_at(plan.id.clone(), 1).unwrap();
    assert_eq!(before.plan.unwrap().total_cents, 134_800);
    assert_eq!(e.item(plan.id.clone()).unwrap().version, 2);
    assert!(e.item_at(plan.id, 0).is_err());
    let space = find_space(&e, "Paraty com a Marina");
    let page = e
        .page_import_markdown(space.id, "notes.md".into(), "# Original\n\nBefore".into())
        .unwrap();
    let observed = e.page(page.id.clone()).unwrap();
    let mut blocks = observed.blocks;
    blocks[0].text = "Edited".into();
    let order = blocks.iter().map(|b| b.id.clone()).collect();
    e.page_apply_from(
        page.id.clone(),
        "edit-version-preview".into(),
        observed.edit_context,
        order,
        blocks,
    )
    .unwrap();
    e.page_commit(page.id.clone(), "Edited".into()).unwrap();
    let preview = e.item_at(page.id.clone(), 1).unwrap();
    assert_eq!(preview.title, "Original");
    assert!(preview.text.unwrap().contains("Before"));
    assert_eq!(e.item(page.id).unwrap().title, "Edited");
}
