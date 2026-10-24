//! Full replay is not authority to schedule private attachment bytes.
use super::*;
use crate::sync::Ingest;
use protocol::{Envelope, Sequenced};

#[test]
fn media_replay_requires_current_recipient_and_removal_retires_background_download(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let author = media_peer("media_author");
    let reader = media_peer("media_reader");
    let stranger = media_peer("media_stranger");
    let anonymous = Client::open(":memory:".into(), "en".into()).unwrap();
    let space = media_space(&author, Some(&reader), "Authorized media")?;
    let file = author
        .file_add(
            space.clone(),
            "private.pdf".into(),
            "private.pdf".into(),
            "application/pdf".into(),
            b"%PDF-1.4 authorized only".to_vec(),
            None,
        )
        .unwrap();
    confirm_media_outbox(&author, &space)?;
    for event in author.lock().logs[&space].events() {
        for recipient in [&reader, &stranger, &anonymous] {
            assert!(matches!(
                recipient.lock().ingest(Sequenced {
                    seq: event.seq,
                    prev: event.prev.clone(),
                    hash: event.hash.clone(),
                    env: Envelope::plain(event),
                }),
                Ingest::Applied
            ));
        }
    }
    assert_eq!(reader.lock().wanted_media().len(), 1);
    assert!(stranger.lock().wanted_media().is_empty());
    assert!(anonymous.lock().wanted_media().is_empty());
    assert_eq!(reader.file_bytes(file.id.clone(), None).unwrap(), None);
    let seen = author.lock().logs[&space].len();
    author
        .remove_member(space.clone(), reader.account().unwrap().identity_id)
        .unwrap();
    confirm_media_outbox(&author, &space)?;
    for event in &author.lock().logs[&space].events()[seen..] {
        assert!(matches!(
            reader.lock().ingest(Sequenced {
                seq: event.seq,
                prev: event.prev.clone(),
                hash: event.hash.clone(),
                env: Envelope::plain(event),
            }),
            Ingest::Applied
        ));
    }
    assert!(reader.lock().wanted_media().is_empty());
    assert_eq!(reader.file_bytes(file.id, None).unwrap(), None);
    Ok(())
}
