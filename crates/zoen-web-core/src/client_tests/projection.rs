//! Original whole test bodies, called by their unchanged registered facade.
use super::*;

pub(super) fn conversation_metadata_comes_from_logs_and_only_my_personal_log_controls_preferences(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, other, mut chat, _) = web_fixture()?;
    clear(
        &mut chat,
        &author,
        EventBody::MemberAdded {
            identity: other.identity.clone(),
            role: Role::Reader,
        },
        3,
    )?;
    let prefs = types::ChatPrefs {
        pinned: true,
        archived: true,
        muted_until_ms: Some(i64::MAX),
    };
    let body = || EventBody::ChatPrefsSet {
        space: chat.space().into(),
        prefs: prefs.clone(),
        operation: None,
    };
    let mut personal = SpaceLog::new("sp_my_personal");
    clear(
        &mut personal,
        &author,
        EventBody::SpaceCreated {
            title: "Personal".into(),
            kind: SpaceKind::Personal,
            privacy: Privacy::EndToEnd,
        },
        4,
    )?;
    clear(&mut personal, &author, body(), 5)?;
    clear(
        &mut personal,
        &other,
        EventBody::ChatPrefsSet {
            space: chat.space().into(),
            prefs: Default::default(),
            operation: None,
        },
        6,
    )?;
    client.logs.insert(personal.space().into(), personal);
    let mut impostor = SpaceLog::new("sp_someone_elses_personal");
    clear(
        &mut impostor,
        &other,
        EventBody::SpaceCreated {
            title: "Other".into(),
            kind: SpaceKind::Personal,
            privacy: Privacy::EndToEnd,
        },
        7,
    )?;
    clear(
        &mut impostor,
        &author,
        EventBody::ChatPrefsSet {
            space: chat.space().into(),
            prefs: Default::default(),
            operation: None,
        },
        8,
    )?;
    client.logs.insert(impostor.space().into(), impostor);
    let profile = Identity {
        id: other.identity.clone(),
        kind: types::IdentityKind::Agent,
        name: "Garden".into(),
        handle: "garden".into(),
        tint_hex: "#34752A".into(),
        glyph: Some("leaf".into()),
        owner: Some(author.identity.clone()),
        bio: "Agent".into(),
        owner_proof: None,
    };
    client
        .saved
        .profiles
        .insert(profile.id.clone(), profile.clone());
    client.logs.insert(chat.space().into(), chat.clone());
    let conversations = client.conversations();
    assert_eq!(conversations.len(), 1);
    let conversation = &conversations[0];
    assert_eq!(conversation.privacy, Privacy::EndToEnd);
    assert_eq!(conversation.roster[&other.identity], Role::Reader);
    assert_eq!(
        conversation
            .profiles
            .iter()
            .map(|profile| profile.identity.clone())
            .collect::<Vec<_>>(),
        vec![profile]
    );
    assert_eq!(conversation.event_count, chat.next_seq().to_string());
    assert!(conversation.prefs.pinned && conversation.prefs.archived);
    assert_eq!(
        conversation.prefs.muted_until_ms.as_deref(),
        Some("9223372036854775807")
    );

    // The same preference body in the group has no effect.
    let chat_id = chat.space().to_owned();
    clear(
        &mut chat,
        &author,
        EventBody::ChatPrefsSet {
            space: chat_id,
            prefs: Default::default(),
            operation: None,
        },
        9,
    )?;
    client.logs.insert(chat.space().into(), chat);
    assert!(client.conversations()[0].prefs.pinned);
    for privacy in [Privacy::Closed, Privacy::Public] {
        let space = format!("sp_{privacy:?}");
        let mut log = SpaceLog::new(&space);
        clear(
            &mut log,
            &author,
            EventBody::SpaceCreated {
                title: space.clone(),
                kind: SpaceKind::Community,
                privacy,
            },
            10,
        )?;
        client.logs.insert(space.clone(), log);
        let projected = client
            .conversations()
            .into_iter()
            .find(|chat| chat.id == space)
            .unwrap();
        assert_eq!(projected.privacy, privacy);
        assert!(!projected.prefs.pinned);
    }
    Ok(())
}

pub(super) fn web_reaction_participants_replace_remove_and_ignore_invalid_reactions(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, other, mut log, target) = web_fixture()?;
    let react = |emoji: &str| types::Annotation::React {
        emoji: emoji.into(),
    };
    annotate(&mut log, &author, &target, react("👍"))?;
    annotate(&mut log, &other, &target, react("👍"))?;
    client.logs.insert(log.space().into(), log.clone());
    let view = client.conversations();
    let reaction = &view[0].messages[0].reactions[0];
    assert_eq!(reaction.count, 2);
    assert_eq!(reaction.participants.len(), 2);
    assert!(reaction.mine);
    assert!(reaction
        .participants
        .iter()
        .any(|p| p.identity == client.account.identity && p.name == "Fixture"));
    annotate(&mut log, &author, &target, react("❤️"))?;
    annotate(&mut log, &other, &target, react(" "))?;
    annotate(&mut log, &other, &target, react(&"😀".repeat(9)))?;
    client.logs.insert(log.space().into(), log.clone());
    let view = client.conversations();
    assert_eq!(view[0].messages[0].reactions.len(), 2);
    annotate(&mut log, &author, &target, types::Annotation::Unreact)?;
    client.logs.insert(log.space().into(), log.clone());
    let view = client.conversations();
    let reactions = &view[0].messages[0].reactions;
    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].emoji, "👍");
    assert_eq!(reactions[0].count, 1);
    assert!(!reactions[0].mine);
    annotate(&mut log, &author, &target, types::Annotation::Delete)?;
    annotate(&mut log, &other, &target, react("❤️"))?;
    client.logs.insert(log.space().into(), log);
    assert!(client.conversations()[0].messages[0].reactions.is_empty());
    assert!(client
        .queue_annotation("sp_web_receipts", &target, react("👍"), 90)
        .err()
        .unwrap()
        .contains("no longer available"));
    Ok(())
}

pub(super) fn web_reaction_submission_rejects_blank_and_oversized_utf8_before_encryption(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, _, log, target) = web_fixture()?;
    client.logs.insert(log.space().into(), log);
    for emoji in [" ".into(), "😀".repeat(9)] {
        assert!(client
            .queue_annotation(
                "sp_web_receipts",
                &target,
                types::Annotation::React { emoji },
                90
            )
            .err()
            .unwrap()
            .contains("32 bytes"));
    }
    Ok(())
}

pub(super) fn browser_directory_never_trusts_person_fields_or_reassigns_a_proven_agent_owner(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, owner, _) = document_fixture()?;
    let person = NativeSigner::from_secret(&[81; 32]);
    let mut record = Identity {
        id: person.id(),
        kind: types::IdentityKind::Person,
        name: "Directory must not reveal this".into(),
        handle: "person".into(),
        tint_hex: "#123456".into(),
        glyph: None,
        owner: None,
        bio: "Private directory text".into(),
        owner_proof: None,
    };
    let cached = client
        .directory_profiles(&serde_json::to_string(&vec![record.clone()]).unwrap())
        .unwrap();
    assert_eq!(cached.result[0]["name"], "@person");
    assert_eq!(cached.result[0]["bio"], "");
    let agent = NativeSigner::from_secret(&[82; 32]);
    record.id = agent.id();
    record.kind = types::IdentityKind::Agent;
    record.name = "Verified agent".into();
    record.owner = Some(owner.identity.clone());
    // Public ownership metadata without the certified device proof has no authority.
    client
        .directory_profiles(&serde_json::to_string(&vec![record.clone()]).unwrap())
        .unwrap();
    assert!(!client.saved.profiles.contains_key(&agent.id()));
    record.owner_proof = Some(Box::new(
        event_log::agent_owner::authorize(&owner, &agent.id()).unwrap(),
    ));
    client
        .directory_profiles(&serde_json::to_string(&vec![record.clone()]).unwrap())
        .unwrap();
    assert_eq!(
        client.saved.profiles[&agent.id()].owner.as_deref(),
        Some(owner.identity.as_str())
    );
    let other = Author::device(
        &NativeSigner::from_secret(&[83; 32]),
        NativeSigner::from_secret(&[84; 32]),
    );
    record.owner = Some(other.identity.clone());
    record.owner_proof = Some(Box::new(
        event_log::agent_owner::authorize(&other, &agent.id()).unwrap(),
    ));
    client
        .directory_profiles(&serde_json::to_string(&vec![record]).unwrap())
        .unwrap();
    assert_eq!(
        client.saved.profiles[&agent.id()].owner.as_deref(),
        Some(owner.identity.as_str())
    );
    Ok(())
}

pub(super) fn browser_typing_is_connection_local_and_requires_a_current_writing_member(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, author, mut log) = document_fixture()?;
    let before = serde_json::to_string(&client.saved).unwrap();
    assert!(matches!(
        ClientFrame::decode(&client.typing_frame(log.space(), true).unwrap()).unwrap(),
        ClientFrame::Ephemeral {
            kind: protocol::EphemeralKind::Typing,
            ..
        }
    ));
    assert!(matches!(
        ClientFrame::decode(&client.typing_frame(log.space(), false).unwrap()).unwrap(),
        ClientFrame::Ephemeral {
            kind: protocol::EphemeralKind::StoppedTyping,
            ..
        }
    ));
    assert_eq!(serde_json::to_string(&client.saved).unwrap(), before);
    client.synced = false;
    assert!(client.typing_frame(log.space(), true).unwrap().is_empty());
    client.synced = true;
    let reader = clear(
        &mut log,
        &author,
        EventBody::MemberAdded {
            identity: author.identity.clone(),
            role: Role::Reader,
        },
        10,
    )?;
    client.receive(&reader, 11).unwrap();
    assert!(client.typing_frame(log.space(), true).unwrap().is_empty());
    Ok(())
}
