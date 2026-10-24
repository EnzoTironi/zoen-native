use super::*;

#[test]
fn own_profile_uses_shared_cipher_signed_upload_and_retries_identical_bytes_after_reload() {
    let (mut client, identity, _) = client();
    request(
        &mut client,
        json!({"op":"update_profile","name":"  Depois 🦋  ","bio":"  privada  "}),
    );
    let update = synced(&mut client);
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
    assert!(profile::verify_upload(
        &identity.id(),
        upload.version,
        &upload.ciphertext,
        &upload.signed
    ));
    let (key, version, fields) = client.my_profile_fields().unwrap();
    assert_eq!(version, 2);
    assert_eq!(fields.name, "Depois 🦋");
    assert_eq!(fields.bio, "privada");
    assert_eq!(
        profile::open_profile(&key, &identity.id(), 2, &upload.ciphertext),
        Some(fields)
    );
    assert!(!upload
        .ciphertext
        .windows("Depois".len())
        .any(|bytes| bytes == b"Depois"));
    let mut reload = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &update.snapshot,
    )
    .unwrap();
    let again = synced(&mut reload);
    let retry = again
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
    assert_eq!(retry, upload);
    assert_eq!(
        request(
            &mut reload,
            json!({"op":"profile","identity_id":identity.id()})
        )
        .result["version"],
        "2"
    );
}

#[test]
fn profile_photo_binary_cache_and_upload_barrier_survive_reload() {
    let (mut client, identity, _) = client();
    synced(&mut client);
    let response = client
        .profile_save(
            r#"{"name":"Photo","bio":"Private","photo":"set","mime":"image/jpeg"}"#,
            b"image pixels".to_vec(),
            20,
        )
        .unwrap();
    assert_eq!(response.uploads.len(), 1);
    assert!(!response
        .update
        .as_ref()
        .unwrap()
        .frames
        .iter()
        .any(|frame| matches!(
            ClientFrame::decode(frame),
            Ok(ClientFrame::Req {
                op: Op::PutProfile { .. },
                ..
            })
        )));
    let photo = client.my_profile_fields().unwrap().2.photo.unwrap();
    assert_eq!(
        photo.open(&client.saved.local_chunks[&photo.blob]),
        Some(b"image pixels".to_vec())
    );
    let snapshot = response.update.unwrap().snapshot;
    let mut reload = Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    assert_eq!(reload.pending_profile_uploads(), vec![photo.blob.clone()]);
    let update = synced(&mut reload);
    assert!(!update.frames.iter().any(|frame| matches!(
        ClientFrame::decode(frame),
        Ok(ClientFrame::Req {
            op: Op::PutProfile { .. },
            ..
        })
    )));
    let framed = reload
        .profile_photo_framed(&identity.id(), &photo.sha256)
        .unwrap();
    let length = u32::from_le_bytes(framed[..4].try_into().unwrap()) as usize;
    assert_eq!(&framed[4 + length..], b"image pixels");
    assert!(reload
        .profile_photo_framed(&identity.id(), &"00".repeat(32))
        .is_err());
    assert!(reload
        .profile_photo_framed("other identity", &photo.sha256)
        .is_err());
    let uploaded = reload
        .document_uploaded(std::slice::from_ref(&photo.blob))
        .unwrap();
    assert!(uploaded.frames.iter().any(|frame| matches!(
        ClientFrame::decode(frame),
        Ok(ClientFrame::Req {
            op: Op::PutProfile { .. },
            ..
        })
    )));
    reload.saved.profile.photos.clear();
    let mut tampered = reload.saved.local_chunks[&photo.blob].clone();
    tampered[25] ^= 1;
    assert!(reload
        .profile_photo_received(&identity.id(), &photo.sha256, tampered)
        .is_err());
    assert!(!reload.profile_photo_cached(&photo));
    reload
        .profile_photo_received(
            &identity.id(),
            &photo.sha256,
            reload.saved.local_chunks[&photo.blob].clone(),
        )
        .unwrap();
    assert!(reload.profile_photo_cached(&photo));
    reload
        .profile_save(
            r#"{"name":"Photo","bio":"Private","photo":"remove"}"#,
            Vec::new(),
            30,
        )
        .unwrap();
    assert!(
        reload
            .profile_photo_framed(&identity.id(), &photo.sha256)
            .is_err(),
        "a disposable cache cannot resurrect a removed photo"
    );
}

#[test]
fn handle_changes_redact_private_fields_and_only_apply_matching_acknowledgements() {
    let (mut client, identity, _) = client();
    let before = client.my_profile_fields().unwrap().2;
    let prepared = client.handle_profile("  @Depois_123  ").unwrap();
    let profile: Identity = serde_json::from_str(&prepared).unwrap();
    assert_eq!(profile.handle, "depois_123");
    assert!(profile.name.is_empty());
    assert!(profile.bio.is_empty());
    assert_eq!(client.account.handle, "fixture");
    let mut wrong = profile.clone();
    wrong.id = event_log::Signer::from_secret(&[82; 32]).id();
    assert!(client
        .handle_confirmed(&serde_json::to_string(&wrong).unwrap(), "depois_123")
        .is_err());
    assert_eq!(client.account.handle, "fixture");
    assert!(client.handle_confirmed(&prepared, "different").is_err());
    assert_eq!(client.account.handle, "fixture");
    let update = client.handle_confirmed(&prepared, "depois_123").unwrap();
    assert_eq!(client.account.handle, "depois_123");
    assert_eq!(client.my_profile_fields().unwrap().2, before);
    let mut original = client.account.clone();
    original.handle = "fixture".into();
    let reload = Client::new(original, client.signer.clone(), &update.snapshot).unwrap();
    assert_eq!(reload.account.handle, "depois_123");
    assert_eq!(reload.fields_of(&identity.id()).unwrap().1, before);
}

#[test]
fn certified_native_share_opens_only_matching_signed_profile_and_survives_reload() {
    let (mut client, _, _) = client();
    let identity = NativeSigner::from_secret(&[74; 32]);
    let device = NativeSigner::from_secret(&[75; 32]);
    let author = Author::device(&identity, device);
    let key = ProfileKey::generate();
    let agreement = client.agreement_key().unwrap();
    let space = "sp_profile";
    let mut log = SpaceLog::new(space);
    let create = clear(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Profile".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
        1,
    );
    client.receive(&create, 2).unwrap();
    let share = profile::seal_share(
        &key,
        3,
        &agreement.public_hex(),
        &profile::share_context(space, &identity.id(), &client.account.identity),
    )
    .unwrap();
    let share = clear(
        &mut log,
        &author,
        EventBody::ProfileKeyShared {
            version: 3,
            shares: vec![types::ProfileKeyShare {
                to: client.account.identity.clone(),
                ephemeral: share.ephemeral,
                sealed: share.sealed,
            }],
        },
        3,
    );
    client.receive(&share, 4).unwrap();
    let update = client.receive(&ServerFrame::SyncDone.encode(), 10).unwrap();
    assert!(update.frames.iter().any(|frame| matches!(
        ClientFrame::decode(frame),
        Ok(ClientFrame::Req {
            op: Op::GetProfiles { .. },
            ..
        })
    )));
    let fields = ProfileFields {
        name: "Nome privado".into(),
        bio: "Só contatos".into(),
        photo: None,
    };
    let ciphertext = profile::seal_profile(&key, &identity.id(), 3, &fields);
    let signed = profile::sign_upload(&author, 3, &ciphertext).unwrap();
    let mut forged = SealedProfile {
        identity: identity.id(),
        version: 4,
        ciphertext: ciphertext.clone(),
        signed: signed.clone(),
    };
    client
        .receive(
            &ServerFrame::Res {
                id: client.profile_net.profiles_request.unwrap(),
                result: Ok(Reply::SealedProfiles(vec![
                    forged.clone(),
                    remote_profile(&client, 1, "Antes", false),
                ])),
            }
            .encode(),
            11,
        )
        .unwrap();
    assert!(client.fields_of(&identity.id()).is_none());
    client.profile_changed(&identity.id());
    client.update(Vec::new()).unwrap();
    forged.version = 3;
    let update = client
        .receive(
            &ServerFrame::Res {
                id: client.profile_net.profiles_request.unwrap(),
                result: Ok(Reply::SealedProfiles(vec![forged])),
            }
            .encode(),
            12,
        )
        .unwrap();
    let reload = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &update.snapshot,
    )
    .unwrap();
    assert_eq!(reload.fields_of(&identity.id()), Some((3, fields)));
    assert!(
        request(
            &mut client,
            json!({"op":"profile","identity_id":identity.id()})
        )
        .result["name"]
            == "Nome privado"
    );
}

#[test]
fn failed_recovery_key_lookup_retries_without_downgrading_blocked_member_to_legacy() {
    let (mut client, _, author) = client();
    let other = NativeSigner::from_secret(&[84; 32]);
    let mut log = SpaceLog::new("sp_key_retry");
    client
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::SpaceCreated {
                    title: "Keys".into(),
                    kind: SpaceKind::Group,
                    privacy: Privacy::Closed,
                },
                1,
            ),
            2,
        )
        .unwrap();
    client
        .receive(
            &clear(
                &mut log,
                &author,
                EventBody::MemberAdded {
                    identity: other.id(),
                    role: Role::Member,
                },
                3,
            ),
            4,
        )
        .unwrap();
    client.saved.profile.blocked.insert(other.id());
    let lookup = |update: &Update| {
        update.frames.iter().any(|frame|matches!(ClientFrame::decode(frame).unwrap(),ClientFrame::Req{op:Op::AgreementKeys{ids},..} if ids.contains(&other.id())))
    };
    assert!(
        lookup(&synced(&mut client)),
        "a blocked member still needs an authenticated MLS recovery key"
    );
    let failed = client
        .receive(
            &ServerFrame::Res {
                id: AGREEMENT_LOOKUP,
                result: Err("temporary failure".into()),
            }
            .encode(),
            11,
        )
        .unwrap();
    assert!(!client.profile_net.no_agreement.contains(&other.id()));
    assert!(!lookup(&failed));
    assert!(!lookup(&client.maintenance(5010).unwrap()));
    assert!(lookup(&client.maintenance(5011).unwrap()));
    client
        .receive(
            &ServerFrame::Res {
                id: AGREEMENT_LOOKUP,
                result: Ok(Reply::AgreementKeys(Vec::new())),
            }
            .encode(),
            5012,
        )
        .unwrap();
    assert!(
        client.profile_net.no_agreement.contains(&other.id()),
        "only a successful empty answer establishes legacy absence"
    );
}

#[test]
fn blocking_rotates_profile_key_and_never_reshares_to_blocked_person() {
    let (mut client, _, author) = client();
    let other = NativeSigner::from_secret(&[76; 32]);
    let space = "sp_block";
    let mut log = SpaceLog::new(space);
    let create = clear(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Block".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
        1,
    );
    client.receive(&create, 2).unwrap();
    let add = clear(
        &mut log,
        &author,
        EventBody::MemberAdded {
            identity: other.id(),
            role: Role::Member,
        },
        3,
    );
    client.receive(&add, 4).unwrap();
    client
        .saved
        .profile
        .agreement
        .insert(other.id(), AgreementKey::generate().public_hex());
    let old = client.my_profile_fields().unwrap().0;
    request(
        &mut client,
        json!({"op":"block_person","identity_id":other.id()}),
    );
    assert_ne!(old.to_hex(), client.my_profile_fields().unwrap().0.to_hex());
    let update = synced(&mut client);
    for frame in &update.frames {
        if let ClientFrame::Publish { env } = ClientFrame::decode(frame).unwrap() {
            let event = event_log::event_from_content(
                env.content().to_vec(),
                env.sig,
                env.cert,
                0,
                String::new(),
                String::new(),
            )
            .unwrap();
            if let EventBody::ProfileKeyShared { shares, .. } = event.body {
                assert!(shares.iter().all(|share| share.to != other.id()));
            }
        }
    }
    let update = request(
        &mut client,
        json!({"op":"unblock_person","identity_id":other.id()}),
    )
    .update
    .unwrap();
    let mut found = false;
    for frame in update.frames {
        if let ClientFrame::Publish { env } = ClientFrame::decode(&frame).unwrap() {
            let bytes = sequence(&mut log, env);
            let decoded = ServerFrame::decode(&bytes).unwrap();
            if let ServerFrame::Event { ev } = decoded {
                let event = event_log::event_from_content(
                    ev.env.content().to_vec(),
                    ev.env.sig,
                    ev.env.cert,
                    0,
                    String::new(),
                    String::new(),
                )
                .unwrap();
                if let EventBody::ProfileKeyShared { shares, .. } = event.body {
                    found |= shares.iter().any(|share| share.to == other.id());
                }
            }
        }
    }
    assert!(found);
}
