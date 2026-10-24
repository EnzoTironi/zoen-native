use super::*;

#[test]
fn profile_lookup_timeout_ignores_late_replies_across_retry_and_cold_reopen() {
    let (mut client, _, _) = client();
    client.saved.profile.own_sync = Some(OwnProfileSync::clean(1));
    client.receive(&ServerFrame::SyncDone.encode(), 10).unwrap();
    let first = client.profile_net.profiles_request.unwrap();
    client.profile_changed(&client.account.identity.clone());
    client.view_now_ms = 15_011;
    let second_update = client.update(Vec::new()).unwrap();
    let second = client.profile_net.profiles_request.unwrap();
    assert_ne!(first, second);
    let old = remote_profile(&client, 2, "Stale query result", false);
    client
        .receive(
            &ServerFrame::Res {
                id: first,
                result: Ok(Reply::SealedProfiles(vec![old.clone()])),
            }
            .encode(),
            15_012,
        )
        .unwrap();
    assert_eq!(client.profile_net.profiles_request, Some(second));
    assert!(!client
        .profile_net
        .fetched
        .contains(&client.account.identity));
    assert_eq!(client.my_profile_fields().unwrap().2.name, "Antes");
    let mut reload = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &second_update.snapshot,
    )
    .unwrap();
    reload
        .receive(&ServerFrame::SyncDone.encode(), 20_000)
        .unwrap();
    let third = reload.profile_net.profiles_request.unwrap();
    assert!(third > second);
    reload
        .receive(
            &ServerFrame::Res {
                id: second,
                result: Ok(Reply::SealedProfiles(vec![old])),
            }
            .encode(),
            20_001,
        )
        .unwrap();
    assert_eq!(reload.profile_net.profiles_request, Some(third));
    assert!(!reload
        .profile_net
        .fetched
        .contains(&reload.account.identity));
    let fresh = remote_profile(&reload, 3, "Current query result", false);
    reload
        .receive(
            &ServerFrame::Res {
                id: third,
                result: Ok(Reply::SealedProfiles(vec![fresh])),
            }
            .encode(),
            20_002,
        )
        .unwrap();
    assert_eq!(
        reload.my_profile_fields().unwrap().2.name,
        "Current query result"
    );
    assert_eq!(reload.saved.profile.own_sync.as_ref().unwrap().published, 3);
    assert!(reload
        .saved
        .profile
        .own_sync
        .as_ref()
        .unwrap()
        .pending
        .is_none());
}

#[test]
fn upload_timeout_retries_exact_bytes_and_late_ack_cannot_confirm_a_new_edit() {
    let (mut client, _, _) = client();
    client.saved.profile.own_sync = Some(OwnProfileSync::clean(1));
    synced(&mut client);
    let update = request(
        &mut client,
        json!({"op":"update_profile","name":"Pending two","bio":"bio"}),
    )
    .update
    .unwrap();
    let upload = update
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Req {
                op: Op::PutProfile { profile },
                ..
            } => Some(profile),
            _ => None,
        })
        .unwrap();
    let first = client.profile_net.upload_request.unwrap();
    client.view_now_ms = 15_021;
    let retry = client.update(Vec::new()).unwrap();
    let second = client.profile_net.upload_request.unwrap();
    assert_ne!(first, second);
    let retried = retry
        .frames
        .iter()
        .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Req {
                op: Op::PutProfile { profile },
                ..
            } => Some(profile),
            _ => None,
        })
        .unwrap();
    assert_eq!(retried, upload);
    client
        .receive(
            &ServerFrame::Res {
                id: first,
                result: Ok(Reply::Done),
            }
            .encode(),
            15_022,
        )
        .unwrap();
    assert_eq!(client.profile_net.upload_request, Some(second));
    assert_eq!(
        client.saved.profile.own_sync.as_ref().unwrap().pending,
        Some(2)
    );
    client.profile_changed(&client.account.identity.clone());
    client.update(Vec::new()).unwrap();
    let lookup = client.profile_net.profiles_request.unwrap();
    client
        .receive(
            &ServerFrame::Res {
                id: lookup,
                result: Ok(Reply::SealedProfiles(vec![upload])),
            }
            .encode(),
            15_023,
        )
        .unwrap();
    assert!(client
        .saved
        .profile
        .own_sync
        .as_ref()
        .unwrap()
        .pending
        .is_none());
    assert!(!client.profile_net.upload);
    client
        .profile_request(
            &json!({"op":"update_profile","name":"Pending three","bio":"bio"}).to_string(),
            20_000,
        )
        .unwrap();
    let third = client.profile_net.upload_request.unwrap();
    assert_ne!(second, third);
    client
        .receive(
            &ServerFrame::Res {
                id: second,
                result: Ok(Reply::Done),
            }
            .encode(),
            20_001,
        )
        .unwrap();
    assert_eq!(client.profile_net.upload_request, Some(third));
    assert_eq!(
        client.saved.profile.own_sync.as_ref().unwrap().pending,
        Some(3)
    );
    client
        .receive(
            &ServerFrame::Res {
                id: third,
                result: Ok(Reply::Done),
            }
            .encode(),
            20_002,
        )
        .unwrap();
    assert!(client
        .saved
        .profile
        .own_sync
        .as_ref()
        .unwrap()
        .pending
        .is_none());
    assert_eq!(client.saved.profile.own_sync.as_ref().unwrap().published, 3);
}
