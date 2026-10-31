use super::*;
use client_core::TimelineEntry;
use std::{path::PathBuf, sync::Arc};

struct Human {
    client: Arc<Client>,
    vault: Arc<crate::deckcraft::tests::Vault>,
    directory: PathBuf,
    space: String,
    message: TimelineEntry,
}

fn human() -> Result<Human, Box<dyn std::error::Error>> {
    let directory = std::env::temp_dir().join(types::new_id("cli-message-info")?);
    std::fs::create_dir(&directory)?;
    let client = Client::open(
        directory
            .join("human.sqlite")
            .to_string_lossy()
            .into_owned(),
        "en".into(),
    )?;
    let vault = Arc::new(crate::deckcraft::tests::Vault::default());
    client.create_account(
        "Human".into(),
        "metadata_human".into(),
        "http://127.0.0.1:1".into(),
        vault.clone(),
    )?;
    drop(client);
    // Reuse the existing offline signed-log fixture pattern with only this real human.
    use client_core::SecretVault;
    let key: [u8; 32] = vault
        .load("zoen.identity.v1".into())
        .ok_or("missing fixture identity key")?
        .try_into()
        .map_err(|_| "invalid fixture identity key")?;
    let signer = event_log::Signer::from_secret(&key);
    let space = types::new_id("sp")?;
    let mut log = event_log::SpaceLog::new(&space);
    let at: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?;
    let database = store::Store::open(&directory.join("human.sqlite").to_string_lossy())?;
    database.append_event(log.append(
        &signer,
        at,
        types::EventBody::SpaceCreated {
            title: "Metadata".into(),
            kind: types::SpaceKind::Group,
            privacy: types::Privacy::Closed,
        },
    )?)?;
    let posted = log
        .append(
            &signer,
            at + 1,
            types::EventBody::MessagePosted {
                message: types::new_id("message")?,
                text: "One human message".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
                forwarded_author: None,
            },
        )?
        .clone();
    database.append_event(&posted)?;
    drop(database);
    let client = Client::open(
        directory
            .join("human.sqlite")
            .to_string_lossy()
            .into_owned(),
        "en".into(),
    )?;
    assert!(client.unlock(vault.clone())?);
    let message = client
        .timeline(space.clone())?
        .into_iter()
        .find(|message| message.id == posted.client_id)
        .ok_or("missing signed human message")?;
    Ok(Human {
        client,
        vault,
        directory,
        space,
        message,
    })
}

#[tokio::test]
async fn canonical_metadata_uses_current_human_owner_and_exact_message_without_mutation(
) -> Result<(), Box<dyn std::error::Error>> {
    let Human {
        client,
        vault: _vault,
        directory,
        space,
        message,
    } = human()?;
    let account = client.account().ok_or("missing human account")?;
    let before = client.verify_log(space.clone()).events;
    let value: serde_json::Value =
        serde_json::from_str(&read(&client, &space, &message.id).await?)?;
    assert_eq!(value["scope"]["expected_identity_id"], account.identity_id);
    assert_eq!(value["scope"]["expected_device_id"], account.device_id);
    assert_eq!(value["scope"]["space_id"], space);
    assert_eq!(value["scope"]["message_id"], message.id);
    assert_eq!(value["sent_at_ms"], message.at_ms.to_string());
    assert_eq!(value["author"]["kind"], "Person");
    assert_eq!(value["edited"], false);
    assert!(value["edited_at_ms"].is_null());
    assert!(value["participants"]
        .as_array()
        .ok_or("missing participant array")?
        .is_empty());
    assert_eq!(client.verify_log(space.clone()).events, before);
    assert!(read(&client, &space, "missing-message").await.is_err());
    assert!(read(&client, "missing-space", &message.id).await.is_err());
    drop(client);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[tokio::test]
async fn read_reports_actual_mark_failure_while_peek_still_reads_the_same_timeline(
) -> Result<(), Box<dyn std::error::Error>> {
    use super::super::read::{read as timeline, Options};
    let Human {
        client,
        vault,
        directory,
        space,
        ..
    } = human()?;
    client.mark_read(space.clone())?;
    drop(client);
    let database = store::Store::open(&directory.join("human.sqlite").to_string_lossy())?;
    database.set_meta(&format!("read:{space}"), "invalid-read-prefix")?;
    drop(database);
    let client = Client::open(
        directory
            .join("human.sqlite")
            .to_string_lossy()
            .into_owned(),
        "en".into(),
    )?;
    assert!(client.unlock(vault)?);
    timeline(
        &client,
        &space,
        Options {
            json: true,
            peek: true,
            older: false,
        },
    )
    .await?;
    assert!(client.mark_read(space.clone()).is_err());
    assert!(timeline(
        &client,
        &space,
        Options {
            json: true,
            peek: false,
            older: false
        }
    )
    .await
    .is_err());
    drop(client);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
