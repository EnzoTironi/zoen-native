use super::*;
use crate::client::tests::{account, clear, sequence, HostSigner};
use event_log::{Author, Signer as NativeSigner};

fn fixture() -> std::result::Result<(Client<HostSigner>, SpaceLog), Box<dyn std::error::Error>> {
    let identity = NativeSigner::from_secret(&[81; 32]);
    let signer = HostSigner(NativeSigner::from_secret(&[82; 32]));
    let author = Author::device(&identity, signer.0.clone());
    let mut client = Client::new(account(&identity, &signer.0), signer, "").unwrap();
    let mut log = SpaceLog::new("sp_web_attachment_batch");
    let frame = clear(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Attachments".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
        1,
    )?;
    client.receive(&frame, 2).unwrap();
    client.receive(&ServerFrame::SyncDone.encode(), 3).unwrap();
    Ok((client, log))
}

fn frame(sealed: &crate::media::Sealed) -> Vec<u8> {
    let uploads = sealed
        .uploads
        .iter()
        .map(|upload| serde_json::json!({"blob":upload.blob,"len":upload.sealed.len()}))
        .collect::<Vec<_>>();
    let header =
        serde_json::to_vec(&serde_json::json!({"doc":sealed.doc,"uploads":uploads})).unwrap();
    let mut result = (header.len() as u32).to_le_bytes().to_vec();
    result.extend(header);
    for upload in &sealed.uploads {
        result.extend(&upload.sealed);
    }
    result
}

fn framed(files: &[crate::media::Sealed]) -> Vec<u8> {
    let mut result = Vec::new();
    for file in files {
        let frame = frame(file);
        result.extend((frame.len() as u32).to_le_bytes());
        result.extend(frame);
    }
    result
}

fn files(count: usize) -> Vec<crate::media::Sealed> {
    (0..count)
        .map(|index| {
            crate::media::seal_file(
                &format!("photo-{index}.jpg"),
                "image/jpeg",
                &[index as u8, 4, 9],
                None,
            )
            .unwrap()
        })
        .collect()
}

fn request(client: &mut Client<HostSigner>, space: &str, count: usize) -> serde_json::Value {
    let producer = client.attachment_producer(r#"{"op":"begin"}"#).unwrap();
    let items = (0..count)
        .map(|index| {
            let mutation = client
                .attachment_producer(
                    &serde_json::json!({
                        "op":"issue", "producer_id":producer,
                    })
                    .to_string(),
                )
                .unwrap();
            serde_json::json!({"mutation_id":mutation,"caption":format!("  Photo {index}\n  ")})
        })
        .collect::<Vec<_>>();
    serde_json::json!({"space_id":space,"reply_to":null,"thread":false,"items":items})
}

fn snapshot(client: &Client<HostSigner>) -> String {
    serde_json::to_string(&client.saved).unwrap()
}

#[test]
fn media_contract_batch_media_group_survives_saved_reload_and_shares_one_upload_barrier(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, mut log) = fixture()?;
    let files = files(2);
    let mut request = request(&mut client, log.space(), 2);
    request["options"] = serde_json::json!({"presentation":"file","grouping":"grouped"});
    let (update, results) = client
        .queue_attachment_batch(&request.to_string(), &framed(&files), 10)
        .unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(client.saved.outbox.len(), 4);
    assert_eq!(client.saved.local_chunks.len(), 2);
    assert!(!update
        .frames
        .iter()
        .any(|frame| matches!(ClientFrame::decode(frame), Ok(ClientFrame::Publish { .. }))));
    let uploads = files
        .iter()
        .flat_map(|file| file.uploads.iter().map(|upload| upload.blob.clone()))
        .collect::<Vec<_>>();
    for pending in client.saved.outbox.values() {
        assert_eq!(
            pending.blob_uploads.iter().collect::<BTreeSet<_>>(),
            uploads.iter().collect()
        );
    }
    let mut restored = Client::new(
        client.account.clone(),
        client.signer.clone(),
        &update.snapshot,
    )
    .unwrap();
    assert!(
        restored
            .queue_attachment_batch(&request.to_string(), &framed(&files), 11)
            .is_err(),
        "a restore retires the old producer"
    );
    restored
        .receive(&ServerFrame::SyncDone.encode(), 12)
        .unwrap();
    let partial = restored.document_uploaded(&uploads[..1]).unwrap();
    assert!(!partial
        .frames
        .iter()
        .any(|frame| matches!(ClientFrame::decode(frame), Ok(ClientFrame::Publish { .. }))));
    let ready = restored.document_uploaded(&uploads[1..]).unwrap();
    let mut posts = Vec::new();
    for raw in ready.frames {
        if let ClientFrame::Publish { env } = ClientFrame::decode(&raw).unwrap() {
            if let Some(event) = env.plain_event() {
                if let EventBody::MessagePosted {
                    media: Some(media), ..
                } = &event.body
                {
                    posts.push(media.clone());
                }
            }
            let confirmed = sequence(&mut log, env)?;
            restored.receive(&confirmed, 13).unwrap();
        }
    }
    assert_eq!(posts.len(), 2);
    assert!(posts
        .iter()
        .all(|media| media.presentation == types::AttachmentPresentation::File));
    assert_eq!(
        posts[0].group.as_ref().unwrap().id,
        posts[1].group.as_ref().unwrap().id
    );
    assert_eq!(posts[0].group.as_ref().unwrap().index, 0);
    assert_eq!(posts[1].group.as_ref().unwrap().index, 1);
    assert_eq!(posts[1].group.as_ref().unwrap().count, 2);
    let conversations = restored.conversations();
    for (index, (item, message)) in results.iter().enumerate() {
        let posted = conversations[0]
            .messages
            .iter()
            .find(|entry| &entry.id == message)
            .unwrap();
        assert_eq!(posted.file_item_id.as_ref(), Some(item));
        assert_eq!(posted.file.as_ref().unwrap().mime, "image/jpeg");
        assert_eq!(
            posted.file.as_ref().unwrap().sha256,
            files[index].doc.sha256
        );
        assert_eq!(
            posted.text,
            request["items"][index]["caption"].as_str().unwrap()
        );
        assert_eq!(
            serde_json::to_value(&posted.media).unwrap(),
            serde_json::to_value(&posts[index]).unwrap()
        );
    }
    Ok(())
}

#[test]
fn media_contract_batch_replay_authenticates_new_ciphertext_but_freezes_order_options_captions_and_bytes(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, log) = fixture()?;
    let mut request = request(&mut client, log.space(), 2);
    request["options"] = serde_json::json!({"presentation":"media","grouping":"grouped"});
    let original = files(2);
    let (_, results) = client
        .queue_attachment_batch(&request.to_string(), &framed(&original), 10)
        .unwrap();
    let baseline = snapshot(&client);
    let fresh = files(2);
    assert_ne!(original[0].doc.chunks[0].blob, fresh[0].doc.chunks[0].blob);
    let (_, replay) = client
        .queue_attachment_batch(&request.to_string(), &framed(&fresh), 11)
        .unwrap();
    assert_eq!(replay, results);
    assert_eq!(
        snapshot(&client),
        baseline,
        "retry cannot replace the accepted ciphertext"
    );
    let mut option_drift = request.clone();
    option_drift["options"]["presentation"] = serde_json::json!("file");
    let mut grouping_drift = request.clone();
    grouping_drift["options"]["grouping"] = serde_json::json!("separate");
    let mut caption_drift = request.clone();
    caption_drift["items"][0]["caption"] = serde_json::json!("changed");
    let mut reordered = request.clone();
    reordered["items"].as_array_mut().unwrap().swap(0, 1);
    for changed in [option_drift, grouping_drift, caption_drift, reordered] {
        assert!(client
            .queue_attachment_batch(&changed.to_string(), &framed(&fresh), 12)
            .is_err());
        assert_eq!(snapshot(&client), baseline);
    }
    let changed = vec![
        crate::media::seal_file("photo-0.jpg", "image/jpeg", b"other", None).unwrap(),
        files(2).remove(1),
    ];
    assert!(client
        .queue_attachment_batch(&request.to_string(), &framed(&changed), 12)
        .is_err());
    assert_eq!(snapshot(&client), baseline);
    Ok(())
}

#[test]
fn media_contract_batch_invalid_last_frame_target_options_or_lease_preserve_every_issued_intent(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, log) = fixture()?;
    let request = request(&mut client, log.space(), 2);
    let valid = framed(&files(2));
    let baseline = snapshot(&client);
    let mut corrupt = valid.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    let mut trailing = valid.clone();
    trailing.push(1);
    for bad in [vec![], valid[..3].to_vec(), corrupt, trailing] {
        assert!(client
            .queue_attachment_batch(&request.to_string(), &bad, 10)
            .is_err());
        assert_eq!(snapshot(&client), baseline);
    }
    let mut requests = Vec::new();
    for options in [
        serde_json::json!({"presentation":"file"}),
        serde_json::json!({"presentation":"media","grouping":"grouped","quality":"original"}),
        serde_json::Value::Null,
    ] {
        let mut bad = request.clone();
        bad["options"] = options;
        requests.push(bad);
    }
    let mut duplicate = request.clone();
    duplicate["items"][1] = duplicate["items"][0].clone();
    requests.push(duplicate);
    let mut retired = request.clone();
    retired["items"][1]["mutation_id"] = serde_json::json!("foreign_1");
    requests.push(retired);
    let mut target = request.clone();
    target["reply_to"] = serde_json::json!("missing");
    target["thread"] = serde_json::json!(true);
    requests.push(target);
    let mut unauthorized = request.clone();
    unauthorized["space_id"] = serde_json::json!("sp_missing");
    requests.push(unauthorized);
    let mut invented_group = request.clone();
    invented_group["group_id"] = serde_json::json!("client-picked");
    requests.push(invented_group);
    for bad in requests {
        assert!(client
            .queue_attachment_batch(&bad.to_string(), &valid, 10)
            .is_err());
        assert_eq!(snapshot(&client), baseline);
    }
    client.saved.outbox_sequence = u64::MAX - 3;
    let full = snapshot(&client);
    assert!(client
        .queue_attachment_batch(&request.to_string(), &valid, 10)
        .is_err());
    assert_eq!(
        snapshot(&client),
        full,
        "all four events must fit before accepting any"
    );
    client.saved.outbox_sequence = 0;
    assert_eq!(
        client
            .queue_attachment_batch(&request.to_string(), &valid, 11)
            .unwrap()
            .1
            .len(),
        2
    );
    Ok(())
}

#[test]
fn media_contract_batch_group_count_limits_and_partial_receipts_never_create_partial_albums(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for count in [0, 1, 2, 10, 11] {
        let (mut client, log) = fixture()?;
        let mut request = request(&mut client, log.space(), count);
        request["options"] = serde_json::json!({"presentation":"media","grouping":"grouped"});
        let baseline = snapshot(&client);
        let result =
            client.queue_attachment_batch(&request.to_string(), &framed(&files(count)), 10);
        if count == 2 || count == 10 {
            assert_eq!(result.unwrap().1.len(), count);
        } else {
            assert!(result.is_err());
            assert_eq!(snapshot(&client), baseline);
        }
    }
    let (mut client, log) = fixture()?;
    let request = request(&mut client, log.space(), 2);
    let files = files(2);
    let single = serde_json::json!({"space_id":log.space(),"reply_to":null,"thread":false,
        "caption":request["items"][0]["caption"],"mutation_id":request["items"][0]["mutation_id"]});
    client
        .queue_attachment(&single.to_string(), &frame(&files[0]), 10)
        .unwrap();
    let baseline = snapshot(&client);
    assert!(client
        .queue_attachment_batch(&request.to_string(), &framed(&files), 11)
        .is_err());
    assert_eq!(snapshot(&client), baseline);
    let second = serde_json::json!({"space_id":log.space(),"reply_to":null,"thread":false,
        "caption":request["items"][1]["caption"],"mutation_id":request["items"][1]["mutation_id"]});
    client
        .queue_attachment(&second.to_string(), &frame(&files[1]), 12)
        .unwrap();
    Ok(())
}

#[test]
fn media_contract_single_legacy_receipts_keep_the_exact_fingerprint_and_file_mode_keeps_real_mime(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, log) = fixture()?;
    let batch = request(&mut client, log.space(), 1);
    let mut request = serde_json::json!({"space_id":log.space(),"reply_to":null,"thread":false,
        "mutation_id":batch["items"][0]["mutation_id"],"caption":batch["items"][0]["caption"]});
    let files = files(1);
    let doc = &files[0].doc;
    let (_, item, message) = client
        .queue_attachment(&request.to_string(), &frame(&files[0]), 10)
        .unwrap();
    let legacy = hex::encode(Sha256::digest(
        serde_json::to_vec(&serde_json::json!([
            request["space_id"],
            request["caption"],
            request["reply_to"],
            request["thread"],
            doc.name,
            doc.mime,
            doc.bytes,
            doc.sha256,
            doc.thumb.as_ref().map(|thumb| (&thumb.sha256, thumb.bytes)),
        ]))
        .unwrap(),
    ));
    let mutation = request["mutation_id"].as_str().unwrap();
    let sequence = client.attachment_producer.sequence(mutation).unwrap();
    assert_eq!(
        client.attachment_producer.pending[&sequence]
            .as_ref()
            .unwrap()
            .fingerprint,
        legacy
    );
    request["options"] = serde_json::json!({"presentation":"media","grouping":"separate"});
    let (_, replay_item, replay_message) = client
        .queue_attachment(&request.to_string(), &frame(&files[0]), 11)
        .unwrap();
    assert_eq!((item, message), (replay_item, replay_message));
    request["options"]["presentation"] = serde_json::json!("file");
    assert!(client
        .queue_attachment(&request.to_string(), &frame(&files[0]), 12)
        .is_err());
    let fresh = self::request(&mut client, log.space(), 1);
    request["mutation_id"] = fresh["items"][0]["mutation_id"].clone();
    let (update, _, _) = client
        .queue_attachment(&request.to_string(), &frame(&files[0]), 13)
        .unwrap();
    let view = serde_json::to_value(&update.conversations).unwrap();
    let posted = view[0]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["media"]["presentation"] == "file")
        .unwrap();
    assert_eq!(posted["file"]["mime"], "image/jpeg");
    request["options"]["grouping"] = serde_json::json!("grouped");
    assert!(client
        .queue_attachment(&request.to_string(), &frame(&files[0]), 14)
        .is_err());
    Ok(())
}

#[test]
fn media_contract_received_ordered_fixtures_match_native_projection(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixtures: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../../types/src/media-received-fixtures.json"
    ))
    .unwrap();
    for scenario in fixtures {
        let (mut client, mut log) = fixture()?;
        let identity = NativeSigner::from_secret(&[81; 32]);
        let device = NativeSigner::from_secret(&[82; 32]);
        let author = Author::device(&identity, device);
        let members = scenario["members"].as_array().unwrap();
        let files = files(members.len());
        for (index, file) in files.iter().enumerate() {
            let raw = clear(
                &mut log,
                &author,
                EventBody::ItemCreated {
                    item: format!("fixture-{index}"),
                    kind: types::ItemKind::File,
                    content: types::ItemContent::File(file.doc.clone()),
                    origin: "Fixture".into(),
                },
                4 + index as i64,
            )?;
            client.receive(&raw, 10).unwrap();
        }
        for (index, member) in members.iter().enumerate() {
            let media = types::MessageMedia {
                presentation: if member["presentation"] == "file" {
                    types::AttachmentPresentation::File
                } else {
                    types::AttachmentPresentation::Media
                },
                group: Some(types::MediaGroup {
                    id: format!("album_{}", "a".repeat(64)),
                    index: member["index"].as_u64().unwrap() as u32,
                    count: member["count"].as_u64().unwrap_or(2) as u32,
                }),
            };
            let raw = clear(
                &mut log,
                &author,
                EventBody::MessagePosted {
                    message: format!("post-{index}"),
                    text: format!("fixture-{index}"),
                    attaches: Some(format!("fixture-{index}")),
                    media: Some(media),
                    reply: member["reply"].as_str().map(|to| types::ReplyRef {
                        to: to.into(),
                        thread: false,
                    }),
                    forwarded: member["forwarded"].as_bool().unwrap_or(false),
                },
                20 + index as i64,
            )?;
            client.receive(&raw, 30).unwrap();
        }
        let conversations = client.conversations();
        let accepted = conversations[0]
            .messages
            .iter()
            .filter_map(|message| {
                message
                    .text
                    .strip_prefix("fixture-")
                    .and_then(|index| index.parse::<u64>().ok())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            accepted,
            scenario["accepted"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_u64().unwrap())
                .collect::<Vec<_>>(),
            "{}",
            scenario["name"]
        );
    }
    Ok(())
}
