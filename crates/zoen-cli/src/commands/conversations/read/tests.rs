use super::*;
use crate::{EntryKind, SecretVault};
use std::sync::Arc;

#[test]
fn peek_flags_are_explicit_and_cannot_fetch_history_or_ignore_extra_arguments(
) -> Result<(), Box<dyn std::error::Error>> {
    for args in [
        vec!["space", "--json", "--peek"],
        vec!["--peek", "space", "--json"],
    ] {
        let mut cli = Cli {
            args: args.into_iter().map(String::from).collect(),
        };
        let options = Options::parse(&mut cli).unwrap();
        assert!(options.json && options.peek && !options.older);
        assert_eq!(cli.args, ["space"]);
    }
    for args in [
        vec!["space", "--peek", "--older"],
        vec!["space", "--json", "--json"],
        vec!["space", "unknown"],
        vec![],
    ] {
        let mut cli = Cli {
            args: args.into_iter().map(String::from).collect(),
        };
        assert!(Options::parse(&mut cli).is_err());
    }
    let mut legacy = Cli {
        args: vec!["space".into(), "--older".into()],
    };
    let options = Options::parse(&mut legacy).unwrap();
    assert!(options.older && !options.peek && !options.json);
    Ok(())
}

#[tokio::test]
async fn json_peek_keeps_real_source_ids_seq_thread_and_reply_without_marking_read(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = crate::deckcraft::tests::fixture()?;
    let space = f.client.item(f.base.item_id.clone()).unwrap().space_id;
    let thread = f
        .client
        .send_reply(
            space.clone(),
            "Scoped thread source".into(),
            f.source.clone(),
            true,
        )
        .unwrap();
    let reply = f
        .client
        .send_reply(
            space.clone(),
            "Inline answer".into(),
            thread.id.clone(),
            false,
        )
        .unwrap();
    let before = f.client.space(space.clone()).unwrap().unread;
    assert!(before > 0);
    let events = f.client.verify_log(space.clone()).events;
    let encoded = read(
        &f.client,
        &space,
        Options {
            json: true,
            peek: true,
            older: false,
        },
    )
    .await
    .unwrap();
    let rows: Vec<client_core::TimelineEntry> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(rows, f.client.timeline(space.clone()).unwrap());
    let source = rows.iter().find(|e| e.id == thread.id).unwrap();
    assert_eq!(source.seq, thread.seq);
    assert_eq!(source.in_thread.as_deref(), Some(f.source.as_str()));
    assert!(
        matches!(&source.kind, EntryKind::Message { text, .. } if text == "Scoped thread source")
    );
    assert_eq!(
        rows.iter()
            .find(|e| e.id == reply.id)
            .unwrap()
            .reply_to
            .as_ref()
            .unwrap()
            .id,
        thread.id
    );
    assert_eq!(f.client.space(space.clone()).unwrap().unread, before);
    assert_eq!(f.client.verify_log(space.clone()).events, events);
    read(&f.client, &space, Options::default()).await.unwrap();
    assert_eq!(
        f.client.space(space).unwrap().unread,
        0,
        "legacy read still advances its watermark"
    );
    let path = f.path.clone();
    drop(f);
    std::fs::remove_dir_all(path).unwrap();
    Ok(())
}

#[test]
fn public_member_and_mls_json_use_only_the_existing_public_dtos(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = crate::deckcraft::tests::fixture()?;
    let space = f
        .client
        .create_group_with(
            "Public key locators".into(),
            vec![],
            crate::PrivacyDto::EndToEnd,
        )
        .unwrap();
    let roles = f.client.member_roles(space.clone()).unwrap();
    let projected: serde_json::Value =
        serde_json::from_str(&members_json(&f.client, &space).unwrap()).unwrap();
    assert_eq!(projected["space_id"], space);
    assert_eq!(projected["members"], serde_json::to_value(roles).unwrap());
    let group = f.client.group_keys(space.clone()).unwrap();
    let keys: serde_json::Value =
        serde_json::from_str(&keys_json(&f.client, &space).unwrap()).unwrap();
    assert_eq!(keys["group"], serde_json::to_value(group).unwrap());
    assert_eq!(
        keys["devices"],
        serde_json::to_value(f.client.group_devices(space)).unwrap()
    );
    assert_eq!(keys.as_object().unwrap().len(), 3);
    assert_eq!(keys["group"].as_object().unwrap().len(), 3);
    let path = f.path.clone();
    drop(f);
    std::fs::remove_dir_all(path).unwrap();
    Ok(())
}

#[tokio::test]
async fn diagnostics_refuse_anonymous_missing_and_nonmember_cached_conversations(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = crate::deckcraft::tests::fixture()?;
    let space = f.client.item(f.base.item_id.clone()).unwrap().space_id;
    let anonymous = Client::open(":memory:".into(), "en".into()).unwrap();
    assert!(members_json(&anonymous, &space).is_err());
    assert!(keys_json(&anonymous, &space).is_err());
    assert!(read(
        &anonymous,
        &space,
        Options {
            json: true,
            peek: true,
            older: false
        }
    )
    .await
    .is_err());
    assert!(members_json(&f.client, "missing").is_err());
    // A different real account with the signed public fixture history cached
    // still cannot use that cache as present conversation membership.
    drop(f.client);
    let source = store::Store::open(f.path.join("agent.sqlite").to_str().unwrap()).unwrap();
    let events = source.events(&space).unwrap();
    let profiles: Vec<_> = source
        .identities()
        .unwrap()
        .into_iter()
        .map(|(profile, _)| profile)
        .collect();
    drop(source);
    let foreign_path = f.path.join("foreign.sqlite");
    let foreign_vault = Arc::new(crate::deckcraft::tests::Vault::default());
    let foreign = Client::open(foreign_path.to_str().unwrap().into(), "en".into()).unwrap();
    foreign
        .create_account(
            "Other".into(),
            "other_cached_reader".into(),
            "http://127.0.0.1:1".into(),
            foreign_vault.clone(),
        )
        .unwrap();
    drop(foreign);
    let cache = store::Store::open(foreign_path.to_str().unwrap()).unwrap();
    for profile in profiles {
        cache.put_identity(&profile, None).unwrap();
    }
    for event in events {
        cache.append_event(&event).unwrap();
    }
    drop(cache);
    let foreign = Client::open(foreign_path.to_str().unwrap().into(), "en".into()).unwrap();
    assert!(foreign
        .unlock(foreign_vault as Arc<dyn SecretVault>)
        .unwrap());
    assert!(foreign.member_roles(space.clone()).unwrap().len() >= 2);
    assert!(members_json(&foreign, &space).is_err());
    assert!(keys_json(&foreign, &space).is_err());
    assert!(read(
        &foreign,
        &space,
        Options {
            json: true,
            peek: true,
            older: false
        }
    )
    .await
    .is_err());
    drop(foreign);
    let path = f.path.clone();
    std::fs::remove_dir_all(path).unwrap();
    Ok(())
}
