use super::*;

#[test]
fn media_contract_batch_group_count_limits_and_partial_receipts_never_create_partial_albums(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for count in [0, 1, 2, 10, 11] {
        let (mut client, log) = fixture()?;
        let mut request = request(&mut client, log.space(), count)?;
        request["options"] = serde_json::json!({"presentation":"media","grouping":"grouped"});
        let baseline = snapshot(&client)?;
        let result =
            client.queue_attachment_batch(&request.to_string(), &framed(&files(count)?)?, 10);
        if count == 2 || count == 10 {
            assert_eq!(result?.1.len(), count);
        } else {
            assert!(result.is_err());
            assert_eq!(snapshot(&client)?, baseline);
        }
    }
    let (mut client, log) = fixture()?;
    let request = request(&mut client, log.space(), 2)?;
    let files = files(2)?;
    let single = serde_json::json!({"space_id":log.space(),"reply_to":null,"thread":false,
        "caption":request["items"][0]["caption"],"mutation_id":request["items"][0]["mutation_id"]});
    client.queue_attachment(&single.to_string(), &frame(&files[0])?, 10)?;
    let baseline = snapshot(&client)?;
    assert!(client
        .queue_attachment_batch(&request.to_string(), &framed(&files)?, 11)
        .is_err());
    assert_eq!(snapshot(&client)?, baseline);
    let second = serde_json::json!({"space_id":log.space(),"reply_to":null,"thread":false,
        "caption":request["items"][1]["caption"],"mutation_id":request["items"][1]["mutation_id"]});
    client.queue_attachment(&second.to_string(), &frame(&files[1])?, 12)?;
    Ok(())
}

#[test]
fn media_contract_single_legacy_receipts_keep_the_exact_fingerprint_and_file_mode_keeps_real_mime(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let (mut client, log) = fixture()?;
    let batch = request(&mut client, log.space(), 1)?;
    let mut request = serde_json::json!({"space_id":log.space(),"reply_to":null,"thread":false,
        "mutation_id":batch["items"][0]["mutation_id"],"caption":batch["items"][0]["caption"]});
    let files = files(1)?;
    let doc = &files[0].doc;
    let (_, item, message) =
        client.queue_attachment(&request.to_string(), &frame(&files[0])?, 10)?;
    let legacy = hex::encode(Sha256::digest(serde_json::to_vec(&serde_json::json!([
        request["space_id"],
        request["caption"],
        request["reply_to"],
        request["thread"],
        doc.name,
        doc.mime,
        doc.bytes,
        doc.sha256,
        doc.thumb.as_ref().map(|thumb| (&thumb.sha256, thumb.bytes)),
    ]))?));
    let mutation = request["mutation_id"]
        .as_str()
        .ok_or("missing authenticated test as_str value")?;
    let sequence = client.attachment_producer.sequence(mutation)?;
    assert_eq!(
        client.attachment_producer.pending[&sequence]
            .as_ref()
            .ok_or("missing authenticated test as_ref value")?
            .fingerprint,
        legacy
    );
    request["options"] = serde_json::json!({"presentation":"media","grouping":"separate"});
    let (_, replay_item, replay_message) =
        client.queue_attachment(&request.to_string(), &frame(&files[0])?, 11)?;
    assert_eq!((item, message), (replay_item, replay_message));
    request["options"]["presentation"] = serde_json::json!("file");
    assert!(client
        .queue_attachment(&request.to_string(), &frame(&files[0])?, 12)
        .is_err());
    let fresh = self::request(&mut client, log.space(), 1)?;
    request["mutation_id"] = fresh["items"][0]["mutation_id"].clone();
    let (update, _, _) = client.queue_attachment(&request.to_string(), &frame(&files[0])?, 13)?;
    let view = serde_json::to_value(&update.conversations)?;
    let posted = view[0]["messages"]
        .as_array()
        .ok_or("missing authenticated test as_array value")?
        .iter()
        .find(|message| message["media"]["presentation"] == "file")
        .ok_or("missing authenticated test find value")?;
    assert_eq!(posted["file"]["mime"], "image/jpeg");
    request["options"]["grouping"] = serde_json::json!("grouped");
    assert!(client
        .queue_attachment(&request.to_string(), &frame(&files[0])?, 14)
        .is_err());
    Ok(())
}

#[test]
fn media_contract_received_ordered_fixtures_match_native_projection(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixtures: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../../../types/src/media-received-fixtures.json"
    ))?;
    for scenario in fixtures {
        let (mut client, mut log) = fixture()?;
        let identity = NativeSigner::from_secret(&[81; 32]);
        let device = NativeSigner::from_secret(&[82; 32]);
        let author = Author::device(&identity, device);
        let members = scenario["members"]
            .as_array()
            .ok_or("missing authenticated test as_array value")?;
        let files = files(members.len())?;
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
                4 + i64::try_from(index)?,
            )?;
            client.receive(&raw, 10)?;
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
                    index: u32::try_from(
                        member["index"]
                            .as_u64()
                            .ok_or("fixture media index missing")?,
                    )?,
                    count: u32::try_from(member["count"].as_u64().unwrap_or(2))?,
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
                        quote: None,
                        artifact: None,
                        to: to.into(),
                        thread: false,
                    }),
                    forwarded: member["forwarded"].as_bool().unwrap_or(false),
                },
                20 + i64::try_from(index)?,
            )?;
            client.receive(&raw, 30)?;
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
                .ok_or("missing authenticated test as_array value")?
                .iter()
                .map(|value| value
                    .as_u64()
                    .ok_or("missing authenticated test as_u64 value"))
                .collect::<std::result::Result<Vec<_>, _>>()?,
            "{}",
            scenario["name"]
        );
    }
    Ok(())
}
