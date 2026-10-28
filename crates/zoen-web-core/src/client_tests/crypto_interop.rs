//! Native SQLite/browser interoperability with the unchanged registered facade.
use super::*;
#[path = "crypto_interop/media_cases.rs"]
mod media_cases;
use media_cases::{assert_failed_media_recovery, assert_file_cards, assert_voice_reply_identity};

pub(super) fn native_sqlite_welcome_messages_saved_reload_and_removal_use_shared_mls(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let identity = NativeSigner::from_secret(&[11; 32]);
    let native_key = NativeSigner::from_secret(&[12; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[13; 32]));
    let account = account(&identity, &host.0);
    let author = Author::device(&identity, native_key.clone());
    let mut sqlite = rusqlite::Connection::open_in_memory()?;
    mls::migrate(&mut sqlite)?;
    let native = Device::new(
        &sqlite,
        [14; 32],
        &identity.id(),
        native_key.secret(),
        author
            .cert
            .as_deref()
            .ok_or("missing authenticated test as_deref value")?,
    )?;
    let mut client = Client::new(account.clone(), host.clone(), "")?;
    let space = "sp_native_web_fixture";
    let mut log = SpaceLog::new(space);
    let roster = join_native_browser(&mut client, &native, &author, &mut log, &identity)?;
    native_message_to_browser(&mut client, &native, &author, &mut log)?;
    assert_file_cards(&mut client, space)?;
    assert_voice_reply_identity(&mut client, space)?;
    assert_failed_media_recovery(&mut client, &account, &host, space)?;
    let accepted = browser_message_to_native(&mut client, &native, &account, &roster, &mut log)?;
    assert_cold_offline_recovery(&account, &host, &accepted, space)?;
    assert_removed_browser_loses_epoch(
        &account, &host, &accepted, &native, &author, &mut log, &identity,
    )?;
    Ok(())
}

fn join_native_browser(
    client: &mut Client<HostSigner>,
    native: &Device<'_>,
    author: &Author,
    log: &mut SpaceLog,
    identity: &NativeSigner,
) -> std::result::Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let space = log.space().to_owned();
    let start = client.bootstrap()?;
    let package = start
        .frames
        .iter()
        .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .find_map(|frame| match frame {
            ClientFrame::Req {
                op: Op::PublishKeyPackages { packages, .. },
                ..
            } => packages.into_iter().next(),
            _ => None,
        })
        .ok_or("missing authenticated test find_map value")?;
    let create = clear(
        log,
        author,
        EventBody::SpaceCreated {
            title: "Native fixture".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
        1,
    )?;
    client.receive(&create, 2)?;
    assert!(client.queue_message(&space, "not yet", 3).is_err());
    native.create_group(&space)?;
    let roster = BTreeSet::from([identity.id()]);
    let commit = native.commit(&space, &[package], &BTreeSet::new())?;
    native.open(&space, &commit.commit, &roster, native.leaf())?;
    client.receive(
        &sealed(log, author, SealedKind::Commit, commit.commit, 4)?,
        5,
    )?;
    client.receive(
        &sealed(
            log,
            author,
            SealedKind::Welcome,
            commit.welcome.ok_or("missing authenticated test welcome")?,
            6,
        )?,
        7,
    )?;
    client.receive(&ServerFrame::SyncDone.encode(), 8)?;
    assert_eq!(
        client.device()?.checkpoint(&space)?,
        native.checkpoint(&space)?
    );
    assert!(client.conversations()[0].can_send);

    Ok(roster)
}

fn native_message_to_browser(
    client: &mut Client<HostSigner>,
    native: &Device<'_>,
    author: &Author,
    log: &mut SpaceLog,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let space = log.space().to_owned();
    let inner = author.sign_event(
        &space,
        &types::new_ulid(9)?,
        9,
        log.head(),
        EventBody::MessagePosted {
            message: "msg_native".into(),
            text: "Native to web".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    )?;
    let plaintext = InnerEvent {
        content: inner.content,
        sig: inner.sig,
    }
    .encode();
    let ciphertext = native.seal(&space, &plaintext)?;
    let (content, sig) = author.sign_sealed(
        &space,
        &inner.client_id,
        9,
        inner.seen.as_ref(),
        Sealed::new(SealedKind::Application, SUITE_ID, ciphertext),
    );
    client.receive(
        &sequence(
            log,
            Envelope::new(content, sig, author.cert.clone(), None)
                .ok_or("authenticated test cryptographic evidence missing")?,
        )?,
        10,
    )?;
    assert_eq!(client.conversations()[0].messages[0].text, "Native to web");

    Ok(())
}

fn browser_message_to_native(
    client: &mut Client<HostSigner>,
    native: &Device<'_>,
    account: &LinkedAccount,
    roster: &BTreeSet<String>,
    log: &mut SpaceLog,
) -> std::result::Result<Update, Box<dyn std::error::Error>> {
    let space = log.space().to_owned();
    let sending = client.queue_message(&space, "Web to native", 11)?;
    let env = sending
        .frames
        .iter()
        .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .find_map(|frame| match frame {
            ClientFrame::Publish { env }
                if env
                    .sealed_data()
                    .is_some_and(|(kind, _)| kind == SealedKind::Application) =>
            {
                Some(env)
            }
            _ => None,
        })
        .ok_or("missing authenticated test find_map value")?;
    env.verify()?;
    let (_, ciphertext) = env
        .sealed_data()
        .ok_or("missing authenticated test sealed_data value")?;
    let from = Leaf {
        identity: account.identity.clone(),
        device: account.device.clone(),
        cert: account.cert.clone(),
    };
    let Opened::Application { plaintext, .. } = native.open(&space, ciphertext, roster, &from)?
    else {
        panic!("real MLS application expected")
    };
    let opened = InnerEvent::decode(&plaintext)
        .ok_or("authenticated test cryptographic evidence missing")?;
    let event = event_log::event_from_content(
        opened.content,
        opened.sig,
        Some(account.cert.clone()),
        0,
        String::new(),
        String::new(),
    )?;
    event_log::verify_author(&event)?;
    assert!(matches!(event.body, EventBody::MessagePosted { text, .. } if text == "Web to native"));
    let accepted = client.receive(&sequence(log, env)?, 12)?;
    assert_eq!(accepted.conversations[0].messages.len(), 2);
    assert_eq!(accepted.conversations[0].messages[1].delivery, "confirmed");

    // A cold offline reopen can enqueue with its provisioned MLS group. It must
    // persist the ratchet and outbox without publishing until catch-up finishes.
    Ok(accepted)
}

fn assert_cold_offline_recovery(
    account: &LinkedAccount,
    host: &HostSigner,
    accepted: &Update,
    space: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut offline = Client::new(account.clone(), host.clone(), &accepted.snapshot)?;
    assert!(!offline.synced);
    assert!(offline.conversations()[0].can_send);
    let queued = offline.queue_message(space, "Offline after reopening", 12)?;
    let queued_id = queued.conversations[0]
        .messages
        .last()
        .ok_or("missing authenticated test last value")?
        .id
        .clone();
    assert!(!queued
        .frames
        .iter()
        .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .any(|frame| matches!(frame,
            ClientFrame::Publish { env } if env.client_id() == queued_id
        )));
    let mut offline_reopened = Client::new(account.clone(), host.clone(), &queued.snapshot)?;
    assert_eq!(offline_reopened.conversations()[0].messages.len(), 3);
    assert_eq!(
        offline_reopened.conversations()[0].messages[2].text,
        "Offline after reopening"
    );
    offline_reopened.bootstrap()?;
    let resumed = offline_reopened.receive(&ServerFrame::SyncDone.encode(), 13)?;
    assert_eq!(
        resumed
            .frames
            .iter()
            .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|frame| matches!(frame,
                ClientFrame::Publish { env } if env.client_id() == queued_id
            ))
            .count(),
        1
    );

    Ok(())
}

fn assert_removed_browser_loses_epoch(
    account: &LinkedAccount,
    host: &HostSigner,
    accepted: &Update,
    native: &Device<'_>,
    author: &Author,
    log: &mut SpaceLog,
    identity: &NativeSigner,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let space = log.space().to_owned();
    let mut restored = Client::new(account.clone(), host.clone(), &accepted.snapshot)?;
    restored.bootstrap()?;
    restored.receive(&ServerFrame::SyncDone.encode(), 13)?;
    assert!(restored.conversations()[0].can_send);
    assert_eq!(restored.conversations()[0].messages.len(), 2);
    assert_eq!(
        restored.device()?.checkpoint(&space)?,
        native.checkpoint(&space)?
    );
    restored.receive(
        &clear(
            log,
            author,
            EventBody::MemberRemoved {
                identity: identity.id(),
            },
            14,
        )?,
        15,
    )?;
    assert!(restored.queue_message(&space, "removed", 16).is_err());
    assert_eq!(restored.device()?.group_state(&space)?, GroupState::Missing);
    Ok(())
}

pub(super) fn invitation_queues_a_certified_self_add_with_the_original_code_and_no_plain_message_fallback(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let identity = NativeSigner::from_secret(&[21; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[22; 32]));
    let mut client = Client::new(account(&identity, &host.0), host, "")?;
    assert!(client
        .queue_join("sp_invite", Role::Member, "invalid", 1)
        .is_err());
    let update = client.queue_join("sp_invite", Role::Member, "0123456789", 2)?;
    let env = update
        .frames
        .iter()
        .map(|frame| ClientFrame::decode(frame).map_err(|error| error.to_string()))
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter()
        .find_map(|frame| match frame {
            ClientFrame::Publish { env } => Some(env),
            _ => None,
        })
        .ok_or("missing authenticated test find_map value")?;
    assert_eq!(env.invite.as_deref(), Some("0123456789"));
    env.verify()?;
    assert!(
        matches!(env.plain_event().ok_or("missing authenticated test plain_event value")?.body, EventBody::MemberAdded { identity: ref added, role: Role::Member } if added == &identity.id())
    );
    assert!(client
        .queue_message("sp_invite", "plain text forbidden", 3)
        .is_err());

    Ok(())
}
