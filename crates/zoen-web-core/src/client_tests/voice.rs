//! Native voice projection scenarios with the unchanged registered facade.
use super::*;

use types::{FileDoc, ItemContent, ItemKind, ReplyRef, VoiceMeta};

pub(super) fn native_voice_replies_versions_and_reverts_stay_on_the_original_post(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut fixture = VoiceFixture::new()?;
    fixture.assert_initial()?;
    let (edited, attachment) = fixture.apply_edits()?;
    fixture.assert_edited(&edited)?;
    fixture.assert_reverts(&edited, &attachment)?;
    fixture.assert_deletion_authority()?;
    Ok(())
}

struct VoiceFixture {
    client: Client<HostSigner>,
    author: Author,
    other: Author,
    log: SpaceLog,
    root: String,
    root_alias: String,
    original: FileDoc,
    entry: String,
    thread_entry: String,
    original_ids: Vec<String>,
}

impl VoiceFixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let identity = NativeSigner::from_secret(&[31; 32]);
        let host = HostSigner(NativeSigner::from_secret(&[32; 32]));
        let author = Author::root(identity.clone());
        let other = Author::root(NativeSigner::from_secret(&[33; 32]));
        let mut client = Client::new(account(&identity, &host.0), host, "")?;
        let space = "sp_voice";
        let mut log = SpaceLog::new(space);
        append(
            &mut log,
            &author,
            EventBody::SpaceCreated {
                title: "Voice fixture".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::EndToEnd,
            },
        )?;
        append(
            &mut log,
            &author,
            EventBody::MessagePosted {
                message: "msg_root".into(),
                text: "Bring the tent".into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
        )?;
        let root = log
            .events()
            .last()
            .ok_or("missing authenticated test last value")?
            .hash
            .clone();
        let root_alias = log
            .events()
            .last()
            .ok_or("missing authenticated test last value")?
            .client_id
            .clone();
        let mut original =
            crate::media::seal_file("voice.webm", "audio/webm", b"original", None)?.doc;
        original.path = "VoiceNotes/native.webm".into();
        original.voice = Some(VoiceMeta {
            ms: 3400,
            levels: "08ff80".into(),
            transcript: "See you there".into(),
            reply: Some(ReplyRef::inline(&root)),
            forwarded: false,
        });
        append(&mut log, &author, post("it_voice", original.clone()))?;
        let entry = log
            .events()
            .last()
            .ok_or("missing authenticated test last value")?
            .hash
            .clone();
        let mut threaded = original.clone();
        threaded
            .voice
            .as_mut()
            .ok_or("missing authenticated test as_mut value")?
            .reply = Some(ReplyRef::thread(&root_alias));
        append(&mut log, &author, post("it_thread", threaded))?;
        let thread_entry = log
            .events()
            .last()
            .ok_or("missing authenticated test last value")?
            .hash
            .clone();
        client.logs.insert(space.into(), log.clone());

        let original_ids = client.conversations()[0]
            .messages
            .iter()
            .map(|message| message.id.clone())
            .collect();
        Ok(Self {
            client,
            author,
            other,
            log,
            root,
            root_alias,
            original,
            entry,
            thread_entry,
            original_ids,
        })
    }
    fn assert_initial(&self) -> Result<(), Box<dyn std::error::Error>> {
        let space = "sp_voice";
        let conversation = &self.client.conversations()[0];
        assert_eq!(conversation.messages.len(), 3);
        let voice = &conversation.messages[1];
        assert_eq!(voice.file.as_ref(), Some(&self.original));
        assert_eq!(
            voice
                .quote
                .as_ref()
                .ok_or("missing authenticated test as_ref value")?
                .id,
            self.root
        );
        assert_eq!(
            voice
                .quote
                .as_ref()
                .ok_or("missing authenticated test as_ref value")?
                .text,
            "Bring the tent"
        );
        assert!(voice.thread.is_none());
        assert!(!voice.edited);
        assert_eq!(
            conversation.messages[2].thread.as_deref(),
            Some(self.root.as_str())
        );
        assert!(conversation.messages[2].quote.is_none());
        assert_eq!(conversation.messages[0].replies, 1);
        assert_eq!(
            self.client
                .posted(space, &self.entry)
                .map(|posted| posted.reply),
            Some(Some(ReplyRef::inline(&self.root)))
        );
        assert_eq!(
            self.client
                .posted(space, &self.thread_entry)
                .map(|posted| posted.reply),
            Some(Some(ReplyRef::thread(&self.root_alias)))
        );

        Ok(())
    }

    fn apply_edits(&mut self) -> Result<(FileDoc, Event), Box<dyn std::error::Error>> {
        let space = "sp_voice";
        let mut edited = crate::media::seal_file("voice.webm", "audio/webm", b"edited", None)?.doc;
        // Native byte edits clear stale timings/transcript but retain the legacy voice path.
        edited.path.clone_from(&self.original.path);
        append(&mut self.log, &self.author, version(edited.clone()))?;
        append(&mut self.log, &self.other, version(self.original.clone()))?;
        // A retained caption/forward must resolve the same current manifest as the
        // original card, including ownership-checked versions and reverts.
        let attachment = self.author.sign_event(
            space,
            "attachment-forward",
            100,
            self.log.head(),
            EventBody::MessagePosted {
                message: "legacy-forward".into(),
                text: "caption".into(),
                attaches: Some("it_voice".into()),
                reply: None,
                media: None,
                forwarded: true,
            },
        )?;
        self.client.logs.insert(space.into(), self.log.clone());
        assert_attachment(&self.client, &attachment, &edited)?;
        Ok((edited, attachment))
    }

    fn assert_edited(&self, edited: &FileDoc) -> Result<(), Box<dyn std::error::Error>> {
        let conversation = &self.client.conversations()[0];
        let voice = &conversation.messages[1];
        assert_eq!(conversation.messages.len(), 4);
        assert_eq!(
            conversation.messages[..3]
                .iter()
                .map(|message| &message.id)
                .collect::<Vec<_>>(),
            self.original_ids.iter().collect::<Vec<_>>()
        );
        let activity = conversation.messages[3]
            .item_edit
            .as_ref()
            .ok_or("missing authenticated test as_ref value")?;
        assert_eq!(activity.item_id, "it_voice");
        assert_eq!(activity.version, 2);
        assert_eq!(activity.note, "Trimmed");
        assert!(!activity.is_undo);
        assert!(conversation.messages[3].file.is_none());
        assert_eq!(conversation.messages[0].replies, 1);
        assert_eq!(voice.entry.as_deref(), Some(self.entry.as_str()));
        assert_eq!(voice.file.as_ref(), Some(edited));
        assert!(voice.edited);
        assert_eq!(
            voice
                .quote
                .as_ref()
                .ok_or("missing authenticated test as_ref value")?
                .id,
            self.root
        );

        Ok(())
    }

    fn assert_reverts(
        &mut self,
        edited: &FileDoc,
        attachment: &Event,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let space = "sp_voice";
        append(&mut self.log, &self.other, revert(1))?;
        append(&mut self.log, &self.author, revert(0))?;
        append(&mut self.log, &self.author, revert(99))?;
        self.client.logs.insert(space.into(), self.log.clone());
        assert_eq!(
            self.client.conversations()[0].messages[1].file.as_ref(),
            Some(edited)
        );
        assert_attachment(&self.client, attachment, edited)?;
        assert_eq!(
            self.client.conversations()[0].messages.len(),
            4,
            "Foreign-self.author and invalid restore targets add no activity"
        );
        append(&mut self.log, &self.author, revert(1))?; // Version 3 is a copy of version 1.
        self.client.logs.insert(space.into(), self.log.clone());
        assert_eq!(
            self.client.conversations()[0].messages[1].file.as_ref(),
            Some(&self.original)
        );
        assert_attachment(&self.client, attachment, &self.original)?;
        append(&mut self.log, &self.author, revert(2))?; // Version 4 is a copy of version 2.
        append(&mut self.log, &self.author, revert(3))?; // Reverts can target earlier reverts.
        self.client.logs.insert(space.into(), self.log.clone());
        let conversation = &self.client.conversations()[0];
        assert_eq!(conversation.messages.len(), 7);
        assert_eq!(
            conversation.messages[..3]
                .iter()
                .map(|message| &message.id)
                .collect::<Vec<_>>(),
            self.original_ids.iter().collect::<Vec<_>>()
        );
        let activities = conversation.messages[3..]
            .iter()
            .map(|message| {
                message
                    .item_edit
                    .as_ref()
                    .ok_or("missing authenticated test as_ref value")
            })
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            activities
                .iter()
                .map(|edit| (edit.version, edit.is_undo))
                .collect::<Vec<_>>(),
            [(2, false), (3, true), (4, true), (5, true)]
        );
        assert!(activities
            .iter()
            .all(|edit| edit.item_id == "it_voice" && edit.space_id == space));
        assert_eq!(conversation.messages[0].replies, 1);
        assert_eq!(
            conversation.messages[1]
                .quote
                .as_ref()
                .ok_or("missing authenticated test as_ref value")?
                .id,
            self.root
        );
        assert_eq!(
            conversation.messages[2].thread.as_deref(),
            Some(self.root.as_str())
        );
        assert_eq!(conversation.messages[1].file.as_ref(), Some(&self.original));
        assert!(conversation.messages[1].edited);
        Ok(())
    }

    fn assert_deletion_authority(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let space = "sp_voice";
        let native_target = self.client.logs[space]
            .events()
            .iter()
            .find(|e| e.hash == self.entry)
            .ok_or("missing authenticated test find value")?
            .client_id
            .clone();
        self.log = self.client.logs[space].clone();
        append(
            &mut self.log,
            &self.other,
            EventBody::MessageAnnotated {
                target: native_target.clone(),
                note: types::Annotation::Delete,
            },
        )?;
        self.client.logs.insert(space.into(), self.log.clone());
        assert!(!self.client.conversations()[0].messages[1].deleted);
        append(
            &mut self.log,
            &self.author,
            EventBody::MessageAnnotated {
                target: native_target,
                note: types::Annotation::Delete,
            },
        )?;
        self.client.logs.insert(space.into(), self.log.clone());
        let conversations = self.client.conversations();
        assert!(conversations[0].messages[1].deleted);
        assert!(conversations[0].messages[1].file.is_none());
        Ok(())
    }
}

fn append(
    log: &mut SpaceLog,
    author: &Author,
    body: EventBody,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = i64::try_from(log.next_seq())? + 1;
    clear(log, author, body, now)?;
    Ok(())
}
fn post(item: &str, doc: FileDoc) -> EventBody {
    EventBody::ItemCreated {
        item: item.into(),
        kind: ItemKind::File,
        content: ItemContent::File(doc),
        origin: "Voice message".into(),
    }
}
fn version(doc: FileDoc) -> EventBody {
    EventBody::ItemVersioned {
        item: "it_voice".into(),
        content: ItemContent::File(doc),
        note: "Trimmed".into(),
    }
}
fn revert(to_version: u32) -> EventBody {
    EventBody::ItemReverted {
        item: "it_voice".into(),
        to_version,
        note: "Restored".into(),
    }
}
fn assert_attachment(
    client: &Client<HostSigner>,
    attachment: &Event,
    expected: &FileDoc,
) -> Result<(), String> {
    let view = client
        .message_view(
            attachment,
            "confirmed",
            None,
            &documents::item_edits(&client.document_items()?),
        )
        .ok_or("missing authenticated test message_view value")?;
    assert_eq!(view.text, "caption");
    assert!(view.forwarded);
    assert_eq!(
        view.file
            .as_ref()
            .ok_or("missing authenticated test as_ref value")?
            .chunks,
        expected.chunks
    );
    assert_eq!(view.file.as_ref(), Some(expected));

    Ok(())
}

pub(super) fn native_forwarded_voice_is_one_destination_only_message_after_receive_and_saved_reload(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    use types::{ItemContent, ItemKind, VoiceMeta};

    for forwarded in [false, true] {
        let identity = NativeSigner::from_secret(&[34; 32]);
        let host = HostSigner(NativeSigner::from_secret(&[35; 32]));
        let author = Author::root(identity.clone());
        let linked = account(&identity, &host.0);
        let mut client = Client::new(linked.clone(), host.clone(), "")?;
        let space = "sp_forward_destination_only";
        let mut log = SpaceLog::new(space);
        let create = clear(
            &mut log,
            &author,
            EventBody::SpaceCreated {
                title: "Destination only".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
            1,
        )?;
        client.receive(&create, 2)?;
        let mut document =
            crate::media::seal_file("forwarded.wav", "audio/wav", b"synthetic audio", None)?.doc;
        document.path = "VoiceNotes/forwarded-destination.wav".into();
        // Old senders omit the flag entirely. New forwarded voices carry it in
        // their one ItemCreated, without a second message or foreign reply IDs.
        let mut wire =
            serde_json::json!({"ms":1500,"levels":"08f","transcript":"Retained transcript"});
        if forwarded {
            wire["forwarded"] = true.into();
        }
        document.voice = Some(serde_json::from_value::<VoiceMeta>(wire)?);
        let posted = clear(
            &mut log,
            &author,
            EventBody::ItemCreated {
                item: "it_fresh_destination_voice".into(),
                kind: ItemKind::File,
                content: ItemContent::File(document.clone()),
                origin: "Voice message".into(),
            },
            3,
        )?;
        let update = client.receive(&posted, 4)?;
        let assert_projection = |client: &Client<HostSigner>| {
            let conversations = client.conversations();
            assert_eq!(conversations.len(), 1);
            let conversation = &conversations[0];
            assert_eq!(conversation.id, space);
            assert_eq!(conversation.messages.len(), 1);
            let message = &conversation.messages[0];
            assert_eq!(message.forwarded, forwarded);
            assert_eq!(message.text, "");
            assert!(message.quote.is_none() && message.thread.is_none());
            assert_eq!(message.replies, 0);
            assert_eq!(
                message.file_item_id.as_deref(),
                Some("it_fresh_destination_voice")
            );
            assert_eq!(message.file.as_ref(), Some(&document));
            assert_eq!(message.file_version, Some(1));
            assert_eq!(client.logs.len(), 1, "no source history is needed");
        };
        assert_projection(&client);
        let restored = Client::new(linked, host, &update.snapshot)?;
        assert_projection(&restored);
    }
    Ok(())
}
