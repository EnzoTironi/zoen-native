//! Serialization errors refuse publication; successful bytes remain wire-compatible.
use super::*;
use serde::{ser::SerializeSeq, Serialize, Serializer};
use types::{Privacy, SpaceKind};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct RefusesAfterPrefix;
impl Serialize for RefusesAfterPrefix {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(2))?;
        sequence.serialize_element("written prefix")?;
        Err(serde::ser::Error::custom("refuse remaining value"))
    }
}

#[test]
fn failed_serializer_does_not_hash_partial_or_substitute_bytes() {
    let error = content_hash(&RefusesAfterPrefix).unwrap_err();
    assert_eq!(error.to_string(), "refuse remaining value");
}

fn creation() -> EventBody {
    EventBody::SpaceCreated {
        title: "Paraty".into(),
        kind: SpaceKind::Group,
        privacy: Privacy::Closed,
    }
}

#[test]
fn sequence_hash_failure_preserves_existing_events_and_head() -> TestResult {
    let author = Author::root(Signer::from_secret(&[7; 32]));
    let mut log = SpaceLog::new("sp_golden");
    log.sequence(author.sign_event("sp_golden", "first", 42, None, creation())?)?;
    let before = serde_json::to_vec(log.events())?;
    let head = log.head();
    let event = author.sign_event("sp_golden", "second", 43, head.clone(), creation())?;
    let error = log
        .sequence_with(event, |_, _, _, _| content_hash(&RefusesAfterPrefix))
        .unwrap_err();
    assert_eq!(error.to_string(), "refuse remaining value");
    assert_eq!(serde_json::to_vec(log.events())?, before);
    assert_eq!(log.head(), head);
    assert_eq!(log.next_seq(), 1);
    Ok(())
}

#[test]
fn empty_log_stays_empty_when_chain_preparation_fails() -> TestResult {
    let author = Author::root(Signer::from_secret(&[7; 32]));
    let mut log = SpaceLog::new("sp_golden");
    let event = author.sign_event("sp_golden", "first", 42, None, creation())?;
    assert!(log
        .sequence_with(event, |_, _, _, _| content_hash(&RefusesAfterPrefix))
        .is_err());
    assert!(log.events().is_empty());
    assert_eq!(log.head(), None);
    assert_eq!(log.head_hash(), GENESIS_PREV);
    Ok(())
}

#[test]
fn serialized_body_signed_content_and_chain_keep_exact_success_bytes() -> TestResult {
    // Independently encoded protobuf/Ed25519 vector, fixed secret [7;32].
    let body = creation();
    assert_eq!(
        content::encode_body(&body)?,
        br#"{"SpaceCreated":{"title":"Paraty","kind":"Group","privacy":"Closed"}}"#
    );
    let author = Author::root(Signer::from_secret(&[7; 32]));
    let event = author.sign_event("sp_golden", "c_golden", 42, None, body)?;
    assert_eq!(
        hex::encode(&event.content),
        concat!(
            "0803120973705f676f6c64656e1a08635f676f6c64656e2240656134613663363365323963353230",
            "6162656635353037623133326563356639393534373736616562656265376239323432316565613639",
            "3134343664323263302a42457b22537061636543726561746564223a7b227469746c65223a225061",
            "72617479222c226b696e64223a2247726f7570222c2270726976616379223a22436c6f736564227d7d"
        )
    );
    assert_eq!(
        content_hash_of(&event),
        "27efaad435409a90d389f7ebd3141b5c85865e3d2a409a64741cf6e70df87e0d"
    );
    assert_eq!(
        event.sig,
        concat!(
            "b4e5ff7ee69e17c82b20129eef25e1e7229f829d1e72fa18c5ad30f30b6c56942",
            "f5ba72170b67c510fcfd5aea22d12adb7e15d7d36962014a49fa2130eb5e107"
        )
    );
    let mut log = SpaceLog::new("sp_golden");
    let event = log.sequence(event)?;
    assert_eq!(
        event.hash,
        "93b4157611bc2f1fcbb3cb1220253b7b7b310c4f99b1c39b574405c2b47c9f66"
    );
    log.verify()?;
    Ok(())
}
