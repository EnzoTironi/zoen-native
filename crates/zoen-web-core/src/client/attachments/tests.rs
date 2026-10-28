use super::*;
use crate::client::tests::{account, clear, sequence, HostSigner};
use crate::client::{ClientFrame, Privacy, ServerFrame, SpaceKind, SpaceLog};
use event_log::{Author, Signer as NativeSigner};

fn fixture() -> std::result::Result<(Client<HostSigner>, SpaceLog), Box<dyn std::error::Error>> {
    let identity = NativeSigner::from_secret(&[81; 32]);
    let signer = HostSigner(NativeSigner::from_secret(&[82; 32]));
    let author = Author::device(&identity, signer.0.clone());
    let mut client = Client::new(account(&identity, &signer.0), signer, "")?;
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
    client.receive(&frame, 2)?;
    client.receive(&ServerFrame::SyncDone.encode(), 3)?;
    Ok((client, log))
}

fn frame(
    sealed: &crate::media::Sealed,
) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error>> {
    let uploads = sealed
        .uploads
        .iter()
        .map(|upload| serde_json::json!({"blob":upload.blob,"len":upload.sealed.len()}))
        .collect::<Vec<_>>();
    let header = serde_json::to_vec(&serde_json::json!({"doc":sealed.doc,"uploads":uploads}))?;
    let mut result = u32::try_from(header.len())?.to_le_bytes().to_vec();
    result.extend(header);
    for upload in &sealed.uploads {
        result.extend(&upload.sealed);
    }
    Ok(result)
}

fn framed(
    files: &[crate::media::Sealed],
) -> std::result::Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut result = Vec::new();
    for file in files {
        let frame = frame(file)?;
        result.extend(u32::try_from(frame.len())?.to_le_bytes());
        result.extend(frame);
    }
    Ok(result)
}

fn files(
    count: usize,
) -> std::result::Result<Vec<crate::media::Sealed>, Box<dyn std::error::Error>> {
    Ok((0..count)
        .map(|index| {
            crate::media::seal_file(
                &format!("photo-{index}.jpg"),
                "image/jpeg",
                &[
                    u8::try_from(index).map_err(|_| "fixture index exceeds byte domain")?,
                    4,
                    9,
                ],
                None,
            )
        })
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

fn request(
    client: &mut Client<HostSigner>,
    space: &str,
    count: usize,
) -> std::result::Result<serde_json::Value, Box<dyn std::error::Error>> {
    let producer = client.attachment_producer(r#"{"op":"begin"}"#)?;
    let items = (0..count)
        .map(|index| {
            let mutation = client
                .attachment_producer(
                    &serde_json::json!({
                        "op":"issue", "producer_id":producer,
                    })
                    .to_string(),
                )
                ?;
            Ok::<_, String>(serde_json::json!({"mutation_id":mutation,"caption":format!("  Photo {index}\n  ")}))
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(serde_json::json!({"space_id":space,"reply_to":null,"thread":false,"items":items}))
}

fn snapshot(
    client: &Client<HostSigner>,
) -> std::result::Result<String, Box<dyn std::error::Error>> {
    Ok(serde_json::to_string(&client.saved)?)
}

fn assert_media_album(posts: &[types::MessageMedia]) -> Result<(), String> {
    assert_eq!(posts.len(), 2);
    assert!(posts
        .iter()
        .all(|media| media.presentation == types::AttachmentPresentation::File));
    assert_eq!(
        posts[0]
            .group
            .as_ref()
            .ok_or("missing authenticated test as_ref value")?
            .id,
        posts[1]
            .group
            .as_ref()
            .ok_or("missing authenticated test as_ref value")?
            .id
    );
    assert_eq!(
        posts[0]
            .group
            .as_ref()
            .ok_or("missing authenticated test as_ref value")?
            .index,
        0
    );
    assert_eq!(
        posts[1]
            .group
            .as_ref()
            .ok_or("missing authenticated test as_ref value")?
            .index,
        1
    );
    assert_eq!(
        posts[1]
            .group
            .as_ref()
            .ok_or("missing authenticated test as_ref value")?
            .count,
        2
    );
    Ok(())
}

#[test]
fn media_contract_batch_media_group_survives_saved_reload_and_shares_one_upload_barrier(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, mut log) = fixture()?;
    let files = files(2)?;
    let mut request = request(&mut client, log.space(), 2)?;
    request["options"] = serde_json::json!({"presentation":"file","grouping":"grouped"});
    let (update, results) =
        client.queue_attachment_batch(&request.to_string(), &framed(&files)?, 10)?;
    assert_eq!(results.len(), 2);
    assert_eq!(client.saved.outbox.len(), 4);
    assert_eq!(client.saved.local_chunks.len(), 2);
    assert!(!update.frames.iter().any(|frame| matches!(
        ClientFrame::decode(frame).map_err(|error| error.to_string()),
        Ok(ClientFrame::Publish { .. })
    )));
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
    )?;
    assert!(
        restored
            .queue_attachment_batch(&request.to_string(), &framed(&files)?, 11)
            .is_err(),
        "a restore retires the old producer"
    );
    restored.receive(&ServerFrame::SyncDone.encode(), 12)?;
    let partial = restored.document_uploaded(&uploads[..1])?;
    assert!(!partial.frames.iter().any(|frame| matches!(
        ClientFrame::decode(frame).map_err(|error| error.to_string()),
        Ok(ClientFrame::Publish { .. })
    )));
    let ready = restored.document_uploaded(&uploads[1..])?;
    let mut posts = Vec::new();
    for raw in ready.frames {
        if let ClientFrame::Publish { env } =
            ClientFrame::decode(&raw).map_err(|error| error.to_string())?
        {
            if let Some(event) = env.plain_event() {
                if let EventBody::MessagePosted {
                    media: Some(media), ..
                } = &event.body
                {
                    posts.push(media.clone());
                }
            }
            let confirmed = sequence(&mut log, env)?;
            restored.receive(&confirmed, 13)?;
        }
    }
    assert_media_album(&posts)?;
    let conversations = restored.conversations();
    for (index, (item, message)) in results.iter().enumerate() {
        let posted = conversations[0]
            .messages
            .iter()
            .find(|entry| &entry.id == message)
            .ok_or("missing authenticated test find value")?;
        assert_eq!(posted.file_item_id.as_ref(), Some(item));
        assert_eq!(
            posted
                .file
                .as_ref()
                .ok_or("missing authenticated test as_ref value")?
                .mime,
            "image/jpeg"
        );
        assert_eq!(
            posted
                .file
                .as_ref()
                .ok_or("missing authenticated test as_ref value")?
                .sha256,
            files[index].doc.sha256
        );
        assert_eq!(
            posted.text,
            request["items"][index]["caption"]
                .as_str()
                .ok_or("missing authenticated test as_str value")?
        );
        assert_eq!(
            serde_json::to_value(&posted.media)?,
            serde_json::to_value(&posts[index])?
        );
    }
    Ok(())
}

#[test]
fn media_contract_batch_replay_authenticates_new_ciphertext_but_freezes_order_options_captions_and_bytes(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, log) = fixture()?;
    let mut request = request(&mut client, log.space(), 2)?;
    request["options"] = serde_json::json!({"presentation":"media","grouping":"grouped"});
    let original = files(2)?;
    let (_, results) =
        client.queue_attachment_batch(&request.to_string(), &framed(&original)?, 10)?;
    let baseline = snapshot(&client)?;
    let fresh = files(2)?;
    assert_ne!(original[0].doc.chunks[0].blob, fresh[0].doc.chunks[0].blob);
    let (_, replay) = client.queue_attachment_batch(&request.to_string(), &framed(&fresh)?, 11)?;
    assert_eq!(replay, results);
    assert_eq!(
        snapshot(&client)?,
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
    reordered["items"]
        .as_array_mut()
        .ok_or("missing authenticated test as_array_mut value")?
        .swap(0, 1);
    for changed in [option_drift, grouping_drift, caption_drift, reordered] {
        assert!(client
            .queue_attachment_batch(&changed.to_string(), &framed(&fresh)?, 12)
            .is_err());
        assert_eq!(snapshot(&client)?, baseline);
    }
    let changed = vec![
        crate::media::seal_file("photo-0.jpg", "image/jpeg", b"other", None)?,
        files(2)?.remove(1),
    ];
    assert!(client
        .queue_attachment_batch(&request.to_string(), &framed(&changed)?, 12)
        .is_err());
    assert_eq!(snapshot(&client)?, baseline);
    Ok(())
}

#[test]
fn media_contract_batch_invalid_last_frame_target_options_or_lease_preserve_every_issued_intent(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, log) = fixture()?;
    let request = request(&mut client, log.space(), 2)?;
    let valid = framed(&files(2)?)?;
    let baseline = snapshot(&client)?;
    let mut corrupt = valid.clone();
    *corrupt
        .last_mut()
        .ok_or("missing authenticated test last_mut value")? ^= 1;
    let mut trailing = valid.clone();
    trailing.push(1);
    for bad in [vec![], valid[..3].to_vec(), corrupt, trailing] {
        assert!(client
            .queue_attachment_batch(&request.to_string(), &bad, 10)
            .is_err());
        assert_eq!(snapshot(&client)?, baseline);
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
        assert_eq!(snapshot(&client)?, baseline);
    }
    client.saved.outbox_sequence = u64::MAX - 3;
    let full = snapshot(&client)?;
    assert!(client
        .queue_attachment_batch(&request.to_string(), &valid, 10)
        .is_err());
    assert_eq!(
        snapshot(&client)?,
        full,
        "all four events must fit before accepting any"
    );
    client.saved.outbox_sequence = 0;
    assert_eq!(
        client
            .queue_attachment_batch(&request.to_string(), &valid, 11)?
            .1
            .len(),
        2
    );
    Ok(())
}

mod batch_receipts;
