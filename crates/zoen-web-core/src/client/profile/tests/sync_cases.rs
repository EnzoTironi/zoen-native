use super::*;

#[test]
fn own_remote_update_adopts_without_echo_and_refreshes_on_reconnect(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, _) = client()?;
    client.saved.profile.own_sync = Some(OwnProfileSync::clean(1));
    let initial = client.receive(&ServerFrame::SyncDone.encode(), 10).unwrap();
    assert!(initial.frames.iter().any(|frame| matches!(ClientFrame::decode(frame),Ok(ClientFrame::Req{op:Op::GetProfiles{ids},..}) if ids.contains(&client.account.identity))));
    let (photo, photo_bytes) =
        profile::ProfilePhoto::seal(b"synthetic peer photo", "image/jpeg").unwrap();
    let fields = ProfileFields {
        name: "Other device".into(),
        photo: Some(photo.clone()),
        ..Default::default()
    };
    let mut remote = remote_profile(&client, 2, "Other device", false)?;
    remote.ciphertext = profile::seal_profile(
        &client.my_profile_fields().unwrap().0,
        &client.account.identity,
        2,
        &fields,
    )?;
    remote.signed = profile::sign_upload(
        &Author::device(
            &NativeSigner::from_secret(&[71; 32]),
            NativeSigner::from_secret(&[92; 32]),
        ),
        2,
        &remote.ciphertext,
    )
    .unwrap();
    let applied = receive_own(&mut client, remote);
    assert_eq!(client.my_profile_fields().unwrap().2.name, "Other device");
    assert_eq!(client.saved.profile.own_sync.as_ref().unwrap().published, 2);
    assert!(!applied.frames.iter().any(|frame| is_profile_upload(frame)));
    assert_eq!(
        client.my_profile_fields().unwrap().2.photo,
        Some(photo.clone())
    );
    assert!(!client.profile_photo_cached(&photo));
    let cached = client
        .profile_photo_received(&client.account.identity.clone(), &photo.sha256, photo_bytes)
        .unwrap();
    assert!(client.profile_photo_cached(&photo));
    assert!(!cached.frames.iter().any(|frame| is_profile_upload(frame)));
    let mut reload = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &applied.snapshot,
    )
    .unwrap();
    let reconnect = reload.receive(&ServerFrame::SyncDone.encode(), 20).unwrap();
    assert!(reconnect.frames.iter().any(|frame| matches!(
        ClientFrame::decode(frame),
        Ok(ClientFrame::Req {
            op: Op::GetProfiles { .. },
            ..
        })
    )));
    assert_eq!(reload.my_profile_fields().unwrap().2.name, "Other device");
    assert!(reload
        .saved
        .profile
        .own_sync
        .as_ref()
        .unwrap()
        .pending
        .is_none());
    Ok(())
}

#[test]
fn pending_local_profile_rebases_and_late_ack_preserves_it(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, _) = client()?;
    client.saved.profile.own_sync = Some(OwnProfileSync::clean(1));
    request(
        &mut client,
        json!({"op":"update_profile","name":"Pending local","bio":"local"}),
    );
    let remote = remote_profile(&client, 4, "Remote newer", false)?;
    let applied = receive_own(&mut client, remote);
    assert_eq!(client.my_profile_fields().unwrap().1, 5);
    assert_eq!(client.my_profile_fields().unwrap().2.name, "Pending local");
    reply_upload(&mut client, 2);
    assert_eq!(
        client.saved.profile.own_sync.as_ref().unwrap().pending,
        Some(5)
    );
    assert_eq!(client.saved.profile.own_sync.as_ref().unwrap().published, 4);
    let reload = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &applied.snapshot,
    )
    .unwrap();
    assert_eq!(
        reload.saved.profile.own_sync.as_ref().unwrap().pending,
        Some(5)
    );
    Ok(())
}

#[test]
fn unknown_rotated_key_blocks_old_key_publication_without_losing_local_intent(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, _) = client()?;
    client.saved.profile.own_sync = Some(OwnProfileSync::clean(1));
    request(
        &mut client,
        json!({"op":"update_profile","name":"Pending local","bio":"local"}),
    );
    client
        .saved
        .profile
        .rows
        .get_mut(&client.account.identity)
        .unwrap()
        .seen_version = 100;
    client.saved.profile.own_sync.as_mut().unwrap().pending = Some(100);
    client.synced = true;
    let remote = remote_profile(&client, 8, "Rotated", true)?;
    let update = receive_own(&mut client, remote);
    assert!(client
        .saved
        .profile
        .own_sync
        .as_ref()
        .unwrap()
        .key_unavailable());
    assert_eq!(client.my_profile_fields().unwrap().2.name, "Pending local");
    assert!(!update.frames.iter().any(|frame| is_profile_upload(frame)));
    let old = remote_profile(&client, 2, "Old key", false)?;
    receive_own(&mut client, old);
    reply_upload(&mut client, 2);
    reply_upload(&mut client, 100);
    let higher_old_key = remote_profile(&client, 101, "Higher old key", false)?;
    let update = receive_own(&mut client, higher_old_key);
    let state = client.saved.profile.own_sync.as_ref().unwrap();
    assert_eq!(state.blocked_at_version, Some(8));
    assert_eq!(state.pending, Some(100));
    assert_eq!(client.my_profile_fields().unwrap().2.name, "Pending local");
    assert!(!update.frames.iter().any(|frame| is_profile_upload(frame)));
    let reload = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &update.snapshot,
    )
    .unwrap();
    assert!(reload
        .saved
        .profile
        .own_sync
        .as_ref()
        .unwrap()
        .key_unavailable());
    Ok(())
}

#[test]
fn failed_own_lookup_retains_work_and_uses_bounded_retry(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, _) = client()?;
    client.receive(&ServerFrame::SyncDone.encode(), 10).unwrap();
    client
        .profile_reply(
            client.profile_net.profiles_request.unwrap(),
            &Err("offline".into()),
        )
        .unwrap();
    assert!(!client
        .profile_net
        .fetched
        .contains(&client.account.identity));
    assert_eq!(client.profile_deadline(), Some(5010));
    assert!(!client
        .update(Vec::new())
        .unwrap()
        .frames
        .iter()
        .any(|frame| is_profile_upload(frame)));
    Ok(())
}

#[test]
fn invalid_own_lookup_retries_without_releasing_pending_profile(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, _) = client()?;
    client.saved.profile.own_sync = Some(OwnProfileSync::clean(1));
    request(
        &mut client,
        json!({"op":"update_profile","name":"Local pending","bio":"bio"}),
    );
    client.receive(&ServerFrame::SyncDone.encode(), 10).unwrap();
    let lookup = client.profile_net.profiles_request.unwrap();
    let before = client.my_profile_fields().unwrap();
    let state = client.saved.profile.own_sync.clone();
    let mut forged = remote_profile(&client, 2, "Forged", false)?;
    forged.signed.sig = "00".repeat(64);
    let update = client
        .receive(
            &ServerFrame::Res {
                id: lookup,
                result: Ok(Reply::SealedProfiles(vec![forged])),
            }
            .encode(),
            11,
        )
        .unwrap();
    assert_eq!(client.my_profile_fields().unwrap(), before);
    assert_eq!(client.saved.profile.own_sync, state);
    assert!(!client
        .profile_net
        .fetched
        .contains(&client.account.identity));
    assert_eq!(client.profile_net.profiles_retry, 5011);
    assert!(!update.frames.iter().any(|frame| is_profile_upload(frame)));
    Ok(())
}

#[test]
fn missing_known_publication_retains_own_work_and_pending_intent(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, _) = client()?;
    client.saved.profile.own_sync = Some(OwnProfileSync::clean(1));
    request(
        &mut client,
        json!({"op":"update_profile","name":"Local pending","bio":"bio"}),
    );
    client.receive(&ServerFrame::SyncDone.encode(), 10).unwrap();
    let lookup = client.profile_net.profiles_request.unwrap();
    let update = client
        .receive(
            &ServerFrame::Res {
                id: lookup,
                result: Ok(Reply::SealedProfiles(Vec::new())),
            }
            .encode(),
            11,
        )
        .unwrap();
    assert!(!client
        .profile_net
        .fetched
        .contains(&client.account.identity));
    assert_eq!(client.profile_net.profiles_retry, 5011);
    assert_eq!(client.saved.profile.own_sync.as_ref().unwrap().published, 1);
    assert_eq!(
        client.saved.profile.own_sync.as_ref().unwrap().pending,
        Some(2)
    );
    assert!(!update.frames.iter().any(|frame| is_profile_upload(frame)));
    Ok(())
}

#[test]
fn missing_profile_bootstrap_and_clean_linked_seed_have_distinct_completion(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, _) = client()?;
    client.saved.profile.own_sync = Some(OwnProfileSync::migrate(1, 0));
    client.receive(&ServerFrame::SyncDone.encode(), 10).unwrap();
    let lookup = client.profile_net.profiles_request.unwrap();
    let update = client
        .receive(
            &ServerFrame::Res {
                id: lookup,
                result: Ok(Reply::SealedProfiles(Vec::new())),
            }
            .encode(),
            11,
        )
        .unwrap();
    assert!(client
        .profile_net
        .fetched
        .contains(&client.account.identity));
    assert!(update.frames.iter().any(|frame| is_profile_upload(frame)));
    let (mut linked, _, _) = self::client()?;
    linked.saved.profile.own_sync = Some(OwnProfileSync::default());
    linked.receive(&ServerFrame::SyncDone.encode(), 10).unwrap();
    let lookup = linked.profile_net.profiles_request.unwrap();
    let update = linked
        .receive(
            &ServerFrame::Res {
                id: lookup,
                result: Ok(Reply::SealedProfiles(Vec::new())),
            }
            .encode(),
            11,
        )
        .unwrap();
    assert!(!linked
        .profile_net
        .fetched
        .contains(&linked.account.identity));
    assert_eq!(linked.profile_net.profiles_retry, 5011);
    assert!(!update.frames.iter().any(|frame| is_profile_upload(frame)));
    Ok(())
}

#[test]
fn stale_signed_own_record_cannot_satisfy_publication_floor(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, _, _) = client()?;
    client.saved.profile.own_sync = Some(OwnProfileSync::clean(1));
    request(
        &mut client,
        json!({"op":"update_profile","name":"Local pending","bio":"bio"}),
    );
    client.receive(&ServerFrame::SyncDone.encode(), 10).unwrap();
    let lookup = client.profile_net.profiles_request.unwrap();
    let old = remote_profile(&client, 0, "Old", false)?;
    let update = client
        .receive(
            &ServerFrame::Res {
                id: lookup,
                result: Ok(Reply::SealedProfiles(vec![old])),
            }
            .encode(),
            11,
        )
        .unwrap();
    assert!(!client
        .profile_net
        .fetched
        .contains(&client.account.identity));
    assert_eq!(client.profile_net.profiles_retry, 5011);
    assert_eq!(client.saved.profile.own_sync.as_ref().unwrap().published, 1);
    assert_eq!(
        client.saved.profile.own_sync.as_ref().unwrap().pending,
        Some(2)
    );
    assert!(!update.frames.iter().any(|frame| is_profile_upload(frame)));
    Ok(())
}
