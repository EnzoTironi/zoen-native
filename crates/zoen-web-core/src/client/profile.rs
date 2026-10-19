//! Encrypted profile cache and retry journal. The wire codec and cryptography are
//! shared with native; the host persists every update before sending its frames.
use super::*;
use event_log::profile::{
    self, AgreementKey, DeviceSigned, ProfileFields, ProfileKey, SealedShare,
};
use event_log::profile_sync::{Observe, OwnProfileSync};
use protocol::{AgreementKeyRecord, SealedProfile};
use serde_json::{json, Value};

const AGREEMENT_PUBLISH: u64 = 0x8000_0010;
const AGREEMENT_LOOKUP: u64 = 0x8000_0012;
const BATCH: usize = 500;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Journal {
    rows: BTreeMap<String, Row>,
    blocked: BTreeSet<String>,
    pub(super) agreement: BTreeMap<String, String>,
    published_agreement: String,
    #[serde(default, skip_serializing)]
    uploaded: u64, // read once when migrating an existing journal
    #[serde(default)]
    own_sync: Option<OwnProfileSync>,
    upload: Option<Upload>,
    #[serde(default)]
    pub(super) photos: BTreeMap<String, Vec<u8>>,
    #[serde(default)]
    pub(super) photo_uploads: BTreeSet<String>,
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::super::tests::{account, clear, sequence, HostSigner};
    use super::*;
    use event_log::{Author, Signer as NativeSigner};

    fn client() -> (Client<HostSigner>, NativeSigner, Author) {
        let identity = NativeSigner::from_secret(&[71; 32]);
        let device = NativeSigner::from_secret(&[72; 32]);
        let author = Author::device(&identity, device.clone());
        let mut account = account(&identity, &device);
        account.agreement_secret = hex::encode([73; 32]);
        account.profile_key = ProfileKey::generate().to_hex();
        account.profile_key_version = 1;
        account.profile_seen_version = 1;
        account.profile_fields = hex::encode(
            ProfileFields {
                name: "Antes".into(),
                bio: "bio".into(),
                photo: None,
            }
            .to_bytes(),
        );
        (
            Client::new(account, HostSigner(device), "").unwrap(),
            identity,
            author,
        )
    }

    fn synced(client: &mut Client<HostSigner>) -> Update {
        let mut first = client.receive(&ServerFrame::SyncDone.encode(), 10).unwrap();
        if !client.profile_net.profiles.is_empty() {
            let sync = client.saved.profile.own_sync.as_ref().unwrap();
            let records = if sync.published == 0 && sync.pending.is_some() {
                Vec::new()
            } else {
                vec![remote_profile(
                    client,
                    sync.published.max(1),
                    "Antes",
                    false,
                )]
            };
            client
                .profile_reply(
                    client.profile_net.profiles_request.unwrap(),
                    &Ok(Reply::SealedProfiles(records)),
                )
                .unwrap();
            let mut next = client.update(Vec::new()).unwrap();
            first.frames.append(&mut next.frames);
            next.frames = first.frames;
            next
        } else {
            first
        }
    }

    fn request(client: &mut Client<HostSigner>, command: Value) -> DocumentResponse {
        client.profile_request(&command.to_string(), 20).unwrap()
    }

    fn remote_profile(
        client: &Client<HostSigner>,
        version: u64,
        name: &str,
        different_key: bool,
    ) -> SealedProfile {
        let identity = NativeSigner::from_secret(&[71; 32]);
        let author = Author::device(&identity, NativeSigner::from_secret(&[92; 32]));
        let key = if different_key {
            ProfileKey::generate()
        } else {
            client.my_profile_fields().unwrap().0
        };
        let ciphertext = profile::seal_profile(
            &key,
            &identity.id(),
            version,
            &ProfileFields {
                name: name.into(),
                ..Default::default()
            },
        );
        SealedProfile {
            identity: identity.id(),
            version,
            signed: profile::sign_upload(&author, version, &ciphertext).unwrap(),
            ciphertext,
        }
    }

    fn receive_own(client: &mut Client<HostSigner>, record: SealedProfile) -> Update {
        client.profile_net.profiles = vec![client.account.identity.clone()];
        client.profile_net.profiles_request = Some(client.next_retained_request_id().unwrap());
        client
            .profile_reply(
                client.profile_net.profiles_request.unwrap(),
                &Ok(Reply::SealedProfiles(vec![record])),
            )
            .unwrap();
        client.update(Vec::new()).unwrap()
    }

    fn reply_upload(client: &mut Client<HostSigner>, version: u64) {
        let id = client.next_retained_request_id().unwrap();
        client.profile_net.upload_version = Some(version);
        client.profile_net.upload_request = Some(id);
        client.profile_reply(id, &Ok(Reply::Done)).unwrap();
    }

    fn is_profile_upload(frame: &[u8]) -> bool {
        matches!(
            ClientFrame::decode(frame),
            Ok(ClientFrame::Req {
                op: Op::PutProfile { .. },
                ..
            })
        )
    }

    #[test]
    fn own_remote_update_adopts_without_echo_and_refreshes_on_reconnect() {
        let (mut client, _, _) = client();
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
        let mut remote = remote_profile(&client, 2, "Other device", false);
        remote.ciphertext = profile::seal_profile(
            &client.my_profile_fields().unwrap().0,
            &client.account.identity,
            2,
            &fields,
        );
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
    }

    #[test]
    fn pending_local_profile_rebases_and_late_ack_preserves_it() {
        let (mut client, _, _) = client();
        client.saved.profile.own_sync = Some(OwnProfileSync::clean(1));
        request(
            &mut client,
            json!({"op":"update_profile","name":"Pending local","bio":"local"}),
        );
        let remote = remote_profile(&client, 4, "Remote newer", false);
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
    }

    #[test]
    fn unknown_rotated_key_blocks_old_key_publication_without_losing_local_intent() {
        let (mut client, _, _) = client();
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
        let remote = remote_profile(&client, 8, "Rotated", true);
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
        let old = remote_profile(&client, 2, "Old key", false);
        receive_own(&mut client, old);
        reply_upload(&mut client, 2);
        reply_upload(&mut client, 100);
        let higher_old_key = remote_profile(&client, 101, "Higher old key", false);
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
    }

    #[test]
    fn failed_own_lookup_retains_work_and_uses_bounded_retry() {
        let (mut client, _, _) = client();
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
    }

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

    #[test]
    fn invalid_own_lookup_retries_without_releasing_pending_profile() {
        let (mut client, _, _) = client();
        client.saved.profile.own_sync = Some(OwnProfileSync::clean(1));
        request(
            &mut client,
            json!({"op":"update_profile","name":"Local pending","bio":"bio"}),
        );
        client.receive(&ServerFrame::SyncDone.encode(), 10).unwrap();
        let lookup = client.profile_net.profiles_request.unwrap();
        let before = client.my_profile_fields().unwrap();
        let state = client.saved.profile.own_sync.clone();
        let mut forged = remote_profile(&client, 2, "Forged", false);
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
    }

    #[test]
    fn missing_known_publication_retains_own_work_and_pending_intent() {
        let (mut client, _, _) = client();
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
    }

    #[test]
    fn missing_profile_bootstrap_and_clean_linked_seed_have_distinct_completion() {
        let (mut client, _, _) = client();
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
        let (mut linked, _, _) = self::client();
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
    }

    #[test]
    fn stale_signed_own_record_cannot_satisfy_publication_floor() {
        let (mut client, _, _) = client();
        client.saved.profile.own_sync = Some(OwnProfileSync::clean(1));
        request(
            &mut client,
            json!({"op":"update_profile","name":"Local pending","bio":"bio"}),
        );
        client.receive(&ServerFrame::SyncDone.encode(), 10).unwrap();
        let lookup = client.profile_net.profiles_request.unwrap();
        let old = remote_profile(&client, 0, "Old", false);
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
    }

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
        let mut reload =
            Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
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
}

#[derive(Clone, Serialize, Deserialize)]
struct Row {
    key: String,
    key_version: u64,
    seen_version: u64,
    fields: Option<Vec<u8>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Upload {
    version: u64,
    ciphertext: Vec<u8>,
    signature: String,
}

#[derive(Default)]
pub(super) struct Network {
    publish: bool,
    upload: bool,
    upload_failed: bool,
    agreement: Vec<String>,
    agreement_retry: i64,
    agreement_deadline: i64,
    profiles: Vec<String>,
    pub(super) no_agreement: BTreeSet<String>,
    fetched: BTreeSet<String>,
    profiles_request: Option<u64>,
    profiles_retry: i64,
    profiles_deadline: i64,
    refetch: BTreeSet<String>,
    upload_version: Option<u64>,
    upload_request: Option<u64>,
    upload_deadline: i64,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Profile { identity_id: String },
    UpdateProfile { name: String, bio: String },
    BlockPerson { identity_id: String },
    UnblockPerson { identity_id: String },
    BlockedPeople {},
}

impl<S: Signer> Client<S> {
    pub(super) fn profile_changed(&mut self, identity: &str) {
        if self.profile_net.profiles.iter().any(|id| id == identity) {
            self.profile_net.refetch.insert(identity.into());
        }
        self.profile_net.fetched.remove(identity);
    }
    pub(super) fn load_linked_profile(&mut self) {
        if let Some(handle) = &self.saved.own_handle {
            self.account.handle = handle.clone();
        }
        if !self.saved.profile.rows.contains_key(&self.account.identity)
            && ProfileKey::from_hex(&self.account.profile_key).is_some()
        {
            let fields = hex::decode(&self.account.profile_fields)
                .ok()
                .filter(|bytes| ProfileFields::from_bytes(bytes).is_some())
                .unwrap_or_else(|| {
                    ProfileFields {
                        name: self.account.name.clone(),
                        ..Default::default()
                    }
                    .to_bytes()
                });
            self.saved.profile.own_sync = Some(if self.account.primary == self.account.device {
                OwnProfileSync::migrate(self.account.profile_seen_version, 0)
            } else {
                OwnProfileSync::default()
            });
            self.saved.profile.rows.insert(
                self.account.identity.clone(),
                Row {
                    key: self.account.profile_key.clone(),
                    key_version: self.account.profile_key_version,
                    seen_version: self.account.profile_seen_version,
                    fields: Some(fields),
                },
            );
        }
        if self.saved.profile.own_sync.is_none() {
            if let Some(row) = self.saved.profile.rows.get(&self.account.identity) {
                self.saved.profile.own_sync = Some(OwnProfileSync::migrate(
                    row.seen_version,
                    self.saved.profile.uploaded,
                ));
            }
        }
        self.overlay_saved_profiles();
    }

    /// Prepare the redacted registration payload without changing the local alias.
    pub fn handle_profile(&self, handle: &str) -> Result<String, String> {
        let handle = protocol::normalize_handle(handle)
            .ok_or("Use de 3 a 24 letras, números, pontos ou _. Comece com uma letra.")?;
        let mut profile = self
            .saved
            .profiles
            .get(&self.account.identity)
            .cloned()
            .unwrap_or(Identity {
                id: self.account.identity.clone(),
                kind: types::IdentityKind::Person,
                name: String::new(),
                handle: handle.clone(),
                tint_hex: "#4F7CFF".into(),
                glyph: None,
                owner: None,
                bio: String::new(),
                owner_proof: None,
            });
        if profile.kind != types::IdentityKind::Person {
            return Err("Este perfil não é uma pessoa.".into());
        }
        profile.name.clear();
        profile.bio.clear();
        profile.handle = handle;
        serde_json::to_string(&profile).map_err(|error| error.to_string())
    }

    /// Only a matching Registered response on this authenticated session can rename.
    pub fn handle_confirmed(&mut self, encoded: &str, expected: &str) -> Result<Update, String> {
        let mut profile: Identity =
            serde_json::from_str(encoded).map_err(|error| error.to_string())?;
        let expected = protocol::normalize_handle(expected).ok_or("Nome de usuário inválido.")?;
        if profile.id != self.account.identity
            || profile.kind != types::IdentityKind::Person
            || profile.owner.is_some()
            || profile.owner_proof.is_some()
            || profile.handle != expected
        {
            return Err("O servidor confirmou outro perfil. Tente novamente.".into());
        }
        self.account.handle = expected.clone();
        self.saved.own_handle = Some(expected);
        profile.name.clear();
        profile.bio.clear();
        self.saved.profiles.insert(profile.id.clone(), profile);
        self.overlay_saved_profiles();
        self.update(Vec::new())
    }

    pub(super) fn agreement_key(&self) -> Option<AgreementKey> {
        let secret: [u8; 32] = hex::decode(&self.account.agreement_secret)
            .ok()?
            .try_into()
            .ok()?;
        Some(AgreementKey::from_secret(secret))
    }

    pub(super) fn fields_of(&self, id: &str) -> Option<(u64, ProfileFields)> {
        let row = self.saved.profile.rows.get(id)?;
        Some((
            row.seen_version,
            ProfileFields::from_bytes(row.fields.as_deref()?)?,
        ))
    }

    pub(super) fn profile_link_fields(&self) -> Result<(String, u64, u64, String), String> {
        let row = self
            .saved
            .profile
            .rows
            .get(&self.account.identity)
            .ok_or("Perfil indisponível.")?;
        ProfileKey::from_hex(&row.key).ok_or("Chave de perfil inválida.")?;
        let fields = row.fields.as_deref().ok_or("Perfil indisponível.")?;
        ProfileFields::from_bytes(fields).ok_or("Perfil inválido.")?;
        Ok((
            row.key.clone(),
            row.key_version,
            row.seen_version,
            hex::encode(fields),
        ))
    }

    pub(super) fn my_profile_fields(&self) -> Result<(ProfileKey, u64, ProfileFields), String> {
        let row = self
            .saved
            .profile
            .rows
            .get(&self.account.identity)
            .ok_or("Perfil indisponível. Vincule este navegador novamente.")?;
        Ok((
            ProfileKey::from_hex(&row.key).ok_or("Chave de perfil inválida.")?,
            row.seen_version,
            ProfileFields::from_bytes(row.fields.as_deref().ok_or("Perfil indisponível.")?)
                .ok_or("Perfil inválido.")?,
        ))
    }

    pub(super) fn overlay_saved_profiles(&mut self) {
        for (id, row) in &self.saved.profile.rows {
            let Some(fields) = row.fields.as_deref().and_then(ProfileFields::from_bytes) else {
                continue;
            };
            if let Some(person) = self
                .saved
                .profiles
                .get_mut(id)
                .filter(|person| person.kind == types::IdentityKind::Person)
            {
                person.name = fields.name.clone();
                person.bio = fields.bio.clone();
            }
            if *id == self.account.identity {
                self.account.name = fields.name;
            }
        }
    }

    pub(super) fn write_profile_fields(
        &mut self,
        fields: ProfileFields,
        rotate: bool,
    ) -> Result<(), String> {
        let me = self.account.identity.clone();
        let row = self
            .saved
            .profile
            .rows
            .get_mut(&me)
            .ok_or("Perfil indisponível.")?;
        let sync = self
            .saved
            .profile
            .own_sync
            .as_mut()
            .ok_or("Estado do perfil indisponível.")?;
        let next = sync
            .edit(row.seen_version)
            .ok_or("O perfil atingiu o limite de versões.")?;
        if rotate {
            row.key = ProfileKey::generate().to_hex();
            row.key_version = next;
        }
        row.seen_version = next;
        row.fields = Some(fields.to_bytes());
        if !self.profile_net.upload {
            self.saved.profile.upload = None;
        }
        self.profile_net.upload_failed = false;
        self.overlay_saved_profiles();
        Ok(())
    }

    fn signed_profile_statement(&self, bytes: &[u8]) -> Result<DeviceSigned, String> {
        Ok(DeviceSigned {
            device: self.account.device.clone(),
            cert: self.account.cert.clone(),
            sig: hex::encode(
                self.signer
                    .sign(bytes)
                    .map_err(|error| format!("{error:?}"))?,
            ),
        })
    }

    pub fn profile_request(&mut self, request: &str, now: i64) -> Result<DocumentResponse, String> {
        self.view_now_ms = now;
        if request.len() > 16 * 1024 {
            return Err("Perfil grande demais.".into());
        }
        let command: Command = serde_json::from_str(request).map_err(|error| error.to_string())?;
        let mut changed = false;
        let result = match command {
            Command::Profile { identity_id } => self.profile_value(&identity_id)?,
            Command::BlockedPeople {} => json!(self.saved.profile.blocked),
            Command::UpdateProfile { name, bio } => {
                let name = name.trim();
                let bio = bio.trim();
                if name.is_empty() || name.chars().count() > 64 {
                    return Err("Diga seu nome com até 64 caracteres.".into());
                }
                if bio.chars().count() > 280 {
                    return Err("A bio tem no máximo 280 caracteres.".into());
                }
                let (_, _, mut fields) = self.my_profile_fields()?;
                if fields.name != name || fields.bio != bio {
                    fields.name = name.into();
                    fields.bio = bio.into();
                    self.write_profile_fields(fields, false)?;
                    changed = true;
                }
                Value::Null
            }
            Command::BlockPerson { identity_id } => {
                if identity_id == self.account.identity
                    || event_log::parse_device_key(&identity_id).is_none()
                {
                    return Err("Não dá para bloquear essa pessoa.".into());
                }
                if !self.saved.profile.blocked.contains(&identity_id) {
                    let (_, _, fields) = self.my_profile_fields()?;
                    self.write_profile_fields(fields, true)?;
                    self.saved.profile.blocked.insert(identity_id);
                    changed = true;
                }
                Value::Null
            }
            Command::UnblockPerson { identity_id } => {
                changed = self.saved.profile.blocked.remove(&identity_id);
                Value::Null
            }
        };
        Ok(DocumentResponse {
            result,
            update: if changed {
                Some(self.update(Vec::new())?)
            } else {
                None
            },
            downloads: Vec::new(),
            uploads: Vec::new(),
        })
    }

    fn profile_value(&self, id: &str) -> Result<Value, String> {
        let identity = self.saved.profiles.get(id);
        let fields = self.fields_of(id);
        if identity.is_none() && fields.is_none() && id != self.account.identity {
            return Err("Perfil não encontrado.".into());
        }
        let agent = identity.is_some_and(|identity| identity.kind == types::IdentityKind::Agent);
        let (name, bio, version, photo) = if agent {
            (
                identity.map(|identity| identity.name.clone()),
                identity.map(|identity| identity.bio.clone()),
                0,
                None,
            )
        } else if let Some((version, fields)) = fields {
            (Some(fields.name), Some(fields.bio), version, fields.photo)
        } else {
            (None, None, 0, None)
        };
        Ok(
            json!({"identity_id":id,"handle":identity.map(|identity|identity.handle.as_str()).unwrap_or(if id==self.account.identity {&self.account.handle} else {""}),
            "name":name,"bio":bio,"photo_sha256":photo.as_ref().map(|photo|photo.sha256.clone()),"photo_mime":photo.as_ref().map(|photo|photo.mime.clone()),"photo_ready":photo.as_ref().is_some_and(|photo| self.profile_photo_cached(photo)),"version":version.to_string(),
            "tint_hex":identity.map(|identity|identity.tint_hex.as_str()).unwrap_or("#8E8E93"),"is_me":id==self.account.identity,"blocked":self.saved.profile.blocked.contains(id)}),
        )
    }

    pub(super) fn note_profile_event(&mut self, event: &Event) {
        let EventBody::ProfileKeyShared { shares, .. } = &event.body else {
            return;
        };
        if event.author == self.account.identity {
            return;
        }
        let Some(agreement) = self.agreement_key() else {
            return;
        };
        let Some(share) = shares
            .iter()
            .find(|share| share.to == self.account.identity)
        else {
            return;
        };
        let Some((key, version)) = profile::open_share(
            &agreement,
            &SealedShare {
                ephemeral: share.ephemeral.clone(),
                sealed: share.sealed.clone(),
            },
            &profile::share_context(&event.space, &event.author, &self.account.identity),
        ) else {
            return;
        };
        if self
            .saved
            .profile
            .rows
            .get(&event.author)
            .is_some_and(|row| row.key_version >= version)
        {
            return;
        }
        let previous = self.saved.profile.rows.get(&event.author);
        self.saved.profile.rows.insert(
            event.author.clone(),
            Row {
                key: key.to_hex(),
                key_version: version,
                seen_version: previous.map(|row| row.seen_version).unwrap_or(0),
                fields: previous.and_then(|row| row.fields.clone()),
            },
        );
        self.profile_net.fetched.remove(&event.author);
    }

    pub(super) fn profile_reply(
        &mut self,
        id: u64,
        result: &Result<Reply, String>,
    ) -> Result<bool, String> {
        match id {
            AGREEMENT_PUBLISH => {
                self.profile_net.publish = false;
                if matches!(result, Ok(Reply::Done)) {
                    self.saved.profile.published_agreement = self
                        .agreement_key()
                        .map(|key| key.public_hex())
                        .unwrap_or_default();
                } else {
                    self.profile_net.publish = true;
                }
            }
            request if self.profile_net.upload_request == Some(request) => {
                self.profile_net.upload_request = None;
                self.profile_net.upload_deadline = 0;
                self.profile_net.upload = false;
                let sent_version = self.profile_net.upload_version.take();
                if matches!(result, Ok(Reply::Done)) {
                    if let Some(version) = sent_version {
                        if let Some(sync) = self.saved.profile.own_sync.as_mut() {
                            sync.acknowledge(version);
                        }
                        if self
                            .saved
                            .profile
                            .upload
                            .as_ref()
                            .is_some_and(|upload| upload.version == version)
                        {
                            self.saved.profile.upload = None;
                        }
                    }
                } else if result
                    .as_ref()
                    .err()
                    .is_some_and(|error| error.contains("stale profile version"))
                {
                    self.profile_net.fetched.remove(&self.account.identity);
                    // Retain exact ciphertext until the authenticated lookup can
                    // distinguish lost ACK from another device's publication.
                } else {
                    self.profile_net.upload_failed = true;
                    self.profile_net.upload_deadline = self.view_now_ms.saturating_add(5000);
                }
            }
            AGREEMENT_LOOKUP => {
                let asked = std::mem::take(&mut self.profile_net.agreement);
                self.profile_net.agreement_deadline = 0;
                let records: &[AgreementKeyRecord] = match result {
                    Ok(Reply::AgreementKeys(records)) => records,
                    _ => {
                        self.profile_net.agreement_retry = self.view_now_ms.saturating_add(5000);
                        return Ok(true);
                    }
                };
                let mut found = BTreeSet::new();
                for record in records
                    .iter()
                    .filter(|record| asked.contains(&record.identity))
                {
                    if profile::verify_agreement(&record.identity, &record.public, &record.signed) {
                        self.saved
                            .profile
                            .agreement
                            .insert(record.identity.clone(), record.public.clone());
                        found.insert(record.identity.clone());
                    }
                }
                self.profile_net
                    .no_agreement
                    .extend(asked.into_iter().filter(|id| !found.contains(id)));
            }
            request if self.profile_net.profiles_request == Some(request) => {
                self.profile_net.profiles_request = None;
                let asked = std::mem::take(&mut self.profile_net.profiles);
                self.profile_net.profiles_deadline = 0;
                let records: &[SealedProfile] = match result {
                    Ok(Reply::SealedProfiles(records)) => records,
                    _ => {
                        self.profile_net.profiles_retry = self.view_now_ms.saturating_add(5000);
                        return Ok(true);
                    }
                };
                let own_asked = asked.contains(&self.account.identity);
                let own_present = records
                    .iter()
                    .any(|record| record.identity == self.account.identity);
                let own_sync = self.saved.profile.own_sync.clone();
                let mut own_verified = false;
                self.profile_net.fetched.extend(asked.iter().cloned());
                for id in &asked {
                    if self.profile_net.refetch.remove(id) {
                        self.profile_net.fetched.remove(id);
                    }
                }
                for record in records
                    .iter()
                    .filter(|record| asked.contains(&record.identity))
                {
                    if record.version == 0
                        || record.ciphertext.len() > profile::MAX_CIPHERTEXT
                        || !profile::verify_upload(
                            &record.identity,
                            record.version,
                            &record.ciphertext,
                            &record.signed,
                        )
                    {
                        continue;
                    }
                    if record.identity == self.account.identity {
                        own_verified |= own_sync
                            .as_ref()
                            .is_some_and(|sync| record.version >= sync.published);
                        self.open_own_profile(record)?;
                        continue;
                    }
                    let Some(row) = self.saved.profile.rows.get_mut(&record.identity) else {
                        continue;
                    };
                    if record.version <= row.seen_version || record.version < row.key_version {
                        continue;
                    }
                    let Some(key) = ProfileKey::from_hex(&row.key) else {
                        continue;
                    };
                    if let Some(fields) = profile::open_profile(
                        &key,
                        &record.identity,
                        record.version,
                        &record.ciphertext,
                    ) {
                        row.seen_version = record.version;
                        row.fields = Some(fields.to_bytes());
                    }
                }
                if own_asked {
                    let bootstrap_absent = !own_present
                        && own_sync.as_ref().is_some_and(|sync| {
                            sync.published == 0 && sync.pending.is_some() && !sync.key_unavailable()
                        });
                    if !own_verified && !bootstrap_absent {
                        self.profile_net.fetched.remove(&self.account.identity);
                        self.profile_net.profiles_retry = self.view_now_ms.saturating_add(5000);
                    }
                }
                self.overlay_saved_profiles();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn open_own_profile(&mut self, record: &SealedProfile) -> Result<(), String> {
        let (key, local_version, local_fields) = self.my_profile_fields()?;
        let row = self
            .saved
            .profile
            .rows
            .get(&self.account.identity)
            .ok_or("Perfil indisponível.")?;
        let mut sync = self
            .saved
            .profile
            .own_sync
            .clone()
            .ok_or("Estado do perfil indisponível.")?;
        if record.version < sync.published || record.version < row.key_version {
            return Ok(());
        }
        let Some(fields) =
            profile::open_profile(&key, &record.identity, record.version, &record.ciphertext)
        else {
            sync.cannot_open(record.version);
            self.saved.profile.own_sync = Some(sync);
            return Ok(());
        };
        let exact = self.saved.profile.upload.as_ref().is_some_and(|upload| {
            upload.version == record.version
                && upload.ciphertext == record.ciphertext
                && upload.signature == record.signed.sig
                && record.signed.device == self.account.device
                && record.signed.cert == self.account.cert
        });
        let outcome = sync
            .observe_opened(record.version, exact)
            .ok_or("O perfil atingiu o limite de versões.")?;
        if matches!(outcome, Observe::Acknowledged)
            && self.profile_net.upload_version == Some(record.version)
        {
            self.profile_net.upload = false;
            self.profile_net.upload_request = None;
            self.profile_net.upload_version = None;
            self.profile_net.upload_deadline = 0;
        }
        let changed = match outcome {
            Observe::Adopt => Some((record.version, fields)),
            Observe::Preserve { version } => Some((version.max(local_version), local_fields)),
            Observe::Acknowledged | Observe::Ignore => None,
        };
        if let Some((version, fields)) = changed {
            let row = self
                .saved
                .profile
                .rows
                .get_mut(&self.account.identity)
                .expect("checked");
            row.seen_version = version;
            row.fields = Some(fields.to_bytes());
        }
        if self
            .saved
            .profile
            .upload
            .as_ref()
            .is_some_and(|upload| Some(upload.version) != sync.pending)
        {
            self.saved.profile.upload = None;
        }
        self.saved.profile.own_sync = Some(sync);
        self.profile_net.upload_failed = false;
        Ok(())
    }

    pub(super) fn profile_deadline(&self) -> Option<i64> {
        [
            self.profile_net.agreement_retry,
            self.profile_net.agreement_deadline,
            self.profile_net.profiles_retry,
            self.profile_net.profiles_deadline,
            self.profile_net.upload_deadline,
        ]
        .into_iter()
        .filter(|deadline| *deadline > self.view_now_ms)
        .min()
    }

    pub(super) fn profile_frames(&mut self, frames: &mut Vec<Vec<u8>>) -> Result<(), String> {
        if !self.synced || self.agreement_key().is_none() || self.my_profile_fields().is_err() {
            return Ok(());
        }
        if !self.profile_net.publish {
            let public = self.agreement_key().expect("checked").public_hex();
            if self.saved.profile.published_agreement != public {
                let signed = self.signed_profile_statement(&profile::agreement_message(
                    &self.account.identity,
                    &public,
                ))?;
                frames.push(
                    ClientFrame::Req {
                        id: AGREEMENT_PUBLISH,
                        op: Op::PublishAgreementKey { public, signed },
                    }
                    .encode(),
                );
                self.profile_net.publish = true;
            }
        }
        if self.profile_net.upload_deadline > 0
            && self.profile_net.upload_deadline <= self.view_now_ms
        {
            self.profile_net.upload = false;
            self.profile_net.upload_failed = false;
            self.profile_net.upload_request = None;
            self.profile_net.upload_version = None;
            self.profile_net.upload_deadline = 0;
        }
        let (key, version, fields) = self.my_profile_fields()?;
        if self.saved.profile.photo_uploads.is_empty()
            && !self.profile_net.upload
            && !self.profile_net.upload_failed
            && self.profile_net.fetched.contains(&self.account.identity)
            && self
                .saved
                .profile
                .own_sync
                .as_ref()
                .is_some_and(|sync| sync.pending == Some(version) && !sync.key_unavailable())
        {
            if self
                .saved
                .profile
                .upload
                .as_ref()
                .is_none_or(|upload| upload.version != version)
            {
                let ciphertext =
                    profile::seal_profile(&key, &self.account.identity, version, &fields);
                let signed = self.signed_profile_statement(&profile::upload_message(
                    &self.account.identity,
                    version,
                    &ciphertext,
                ))?;
                self.saved.profile.upload = Some(Upload {
                    version,
                    ciphertext,
                    signature: signed.sig,
                });
            }
            let request = self.next_retained_request_id()?;
            let upload = self.saved.profile.upload.as_ref().expect("prepared");
            frames.push(
                ClientFrame::Req {
                    id: request,
                    op: Op::PutProfile {
                        profile: SealedProfile {
                            identity: self.account.identity.clone(),
                            version: upload.version,
                            ciphertext: upload.ciphertext.clone(),
                            signed: DeviceSigned {
                                device: self.account.device.clone(),
                                cert: self.account.cert.clone(),
                                sig: upload.signature.clone(),
                            },
                        },
                    },
                }
                .encode(),
            );
            self.profile_net.upload = true;
            self.profile_net.upload_version = Some(version);
            self.profile_net.upload_request = Some(request);
            self.profile_net.upload_deadline = self.view_now_ms.saturating_add(15_000);
        }
        self.share_profile_key()?;
        if !self.profile_net.agreement.is_empty()
            && self.profile_net.agreement_deadline <= self.view_now_ms
        {
            self.profile_net.agreement.clear();
        }
        if self.profile_net.agreement.is_empty()
            && self.profile_net.agreement_retry <= self.view_now_ms
        {
            let missing = self
                .logs
                .keys()
                .flat_map(|space| self.roster(space).into_keys())
                .filter(|id| {
                    id != &self.account.identity
                        && !self.saved.profile.agreement.contains_key(id)
                        && !self.profile_net.no_agreement.contains(id)
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(BATCH)
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                frames.push(
                    ClientFrame::Req {
                        id: AGREEMENT_LOOKUP,
                        op: Op::AgreementKeys {
                            ids: missing.clone(),
                        },
                    }
                    .encode(),
                );
                self.profile_net.agreement = missing;
                self.profile_net.agreement_deadline = self.view_now_ms.saturating_add(15_000);
            }
        }
        if !self.profile_net.profiles.is_empty()
            && self.profile_net.profiles_deadline <= self.view_now_ms
        {
            self.profile_net.profiles.clear();
            self.profile_net.profiles_request = None;
        }
        if self.profile_net.profiles.is_empty()
            && self.profile_net.profiles_retry <= self.view_now_ms
        {
            let missing = self
                .saved
                .profile
                .rows
                .keys()
                .filter(|id| !self.profile_net.fetched.contains(*id))
                .take(BATCH)
                .cloned()
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                let request = self.next_retained_request_id()?;
                self.profile_net.profiles_request = Some(request);
                frames.push(
                    ClientFrame::Req {
                        id: request,
                        op: Op::GetProfiles {
                            ids: missing.clone(),
                        },
                    }
                    .encode(),
                );
                for id in &missing {
                    self.profile_net.refetch.remove(id);
                }
                self.profile_net.profiles = missing;
                self.profile_net.profiles_deadline = self.view_now_ms.saturating_add(15_000);
            }
        }
        Ok(())
    }

    fn share_profile_key(&mut self) -> Result<(), String> {
        let row = self
            .saved
            .profile
            .rows
            .get(&self.account.identity)
            .ok_or("Perfil indisponível.")?;
        let key = ProfileKey::from_hex(&row.key).ok_or("Chave de perfil inválida.")?;
        let version = row.key_version;
        let me = &self.account.identity;
        let mut shared = BTreeSet::new();
        let pending = self
            .saved
            .outbox
            .values()
            .filter(|pending| pending.failure.is_none())
            .filter_map(|pending| pending.envelope().ok())
            .filter_map(|envelope| {
                event_log::event_from_content(
                    envelope.content().to_vec(),
                    envelope.sig,
                    envelope.cert,
                    0,
                    String::new(),
                    String::new(),
                )
                .ok()
            })
            .collect::<Vec<_>>();
        for event in self
            .logs
            .values()
            .flat_map(|log| log.events())
            .chain(pending.iter())
        {
            if let EventBody::ProfileKeyShared { version: v, shares } = &event.body {
                if event.author == *me && *v == version {
                    shared.extend(shares.iter().map(|share| share.to.clone()));
                }
            }
        }
        let mut bodies = Vec::new();
        for space in self.logs.keys() {
            let roster = self.roster(space);
            if !roster.contains_key(me) {
                continue;
            }
            let mut shares = Vec::new();
            for id in roster.keys() {
                if id == me
                    || shared.contains(id)
                    || self.saved.profile.blocked.contains(id)
                    || self
                        .saved
                        .profiles
                        .get(id)
                        .is_some_and(|identity| identity.kind == types::IdentityKind::Agent)
                {
                    continue;
                }
                let Some(public) = self.saved.profile.agreement.get(id) else {
                    continue;
                };
                if let Some(sealed) = profile::seal_share(
                    &key,
                    version,
                    public,
                    &profile::share_context(space, me, id),
                ) {
                    shares.push(types::ProfileKeyShare {
                        to: id.clone(),
                        ephemeral: sealed.ephemeral,
                        sealed: sealed.sealed,
                    });
                    shared.insert(id.clone());
                }
            }
            for shares in shares.chunks(200) {
                bodies.push((
                    space.clone(),
                    EventBody::ProfileKeyShared {
                        version,
                        shares: shares.to_vec(),
                    },
                ));
            }
        }
        for (space, body) in bodies {
            let pending = self.clear_event(&space, body, self.view_now_ms)?;
            let id = pending.envelope()?.client_id().to_string();
            self.saved.outbox.insert(id, pending);
        }
        Ok(())
    }
}
