//! Upgrade one inert predecessor view from the author's unchanged signed bytes.
use super::{content, Event, EventBody, Payload, SignedContent};

pub(super) fn hydrate(mut event: Event) -> Event {
    if !matches!(&event.body, EventBody::Unsupported { kind } if kind == "AppEditRequested") {
        return event;
    }
    let Some(signed) = SignedContent::parse(&event.content) else {
        return event;
    };
    let Some(Payload::Body(bytes)) = signed.payload else {
        return event;
    };
    let body = content::decode_body(&bytes);
    // This is a derived view only. from_events still verifies EVERY signature,
    // header, chain link and causal cut before the Core may project the request.
    if matches!(&body, EventBody::AppEditRequested { .. }) {
        event.body = body;
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{verify_author, SpaceLog};

    fn predecessor() -> Vec<Event> {
        serde_json::from_str(include_str!("../testdata/app-edit-target-cohort.json")).unwrap()
    }
    #[test]
    fn genuine_601c_reader_storage_upgrades_without_rewriting_signed_bytes() {
        let provenance: serde_json::Value = serde_json::from_str(include_str!(
            "../testdata/app-edit-target-cohort-provenance.json"
        ))
        .unwrap();
        assert_eq!(
            provenance["predecessor_commit"],
            "601c94a73d6c0277066cbfb54be79065f8d009c4"
        );
        assert_eq!(
            provenance["fixture_sha256"],
            crate::sha256_hex(include_bytes!("../testdata/app-edit-target-cohort.json"))
        );
        assert_eq!(
            provenance["probe_sha256"],
            crate::sha256_hex(include_bytes!(
                "../testdata/app-edit-target-cohort-probe.rs"
            ))
        );
        let old = predecessor();
        assert!(
            matches!(&old[1].body, EventBody::Unsupported { kind } if kind == "AppEditRequested")
        );
        let log = SpaceLog::from_events("sp_target_cohort", old.clone()).unwrap();
        let EventBody::AppEditRequested { reply, .. } = &log.events()[1].body else {
            panic!("upgraded signed request")
        };
        let request = reply.artifact.as_ref().unwrap();
        assert_eq!(request.agent_id.as_deref(), Some("operator"));
        assert_eq!(
            request.intent_sha256.as_deref(),
            Some("ef".repeat(32).as_str())
        );
        for (before, after) in old.iter().zip(log.events()) {
            assert_eq!(before.content, after.content);
            assert_eq!(before.sig, after.sig);
            assert_eq!(before.hash, after.hash);
            assert_eq!(before.prev, after.prev);
            verify_author(after).unwrap();
        }
        let saved = serde_json::to_string(log.events()).unwrap();
        assert!(
            SpaceLog::from_events("sp_target_cohort", serde_json::from_str(&saved).unwrap())
                .is_ok()
        );
        assert!(
            matches!(&log.events()[2].body, EventBody::MessagePosted { text, .. } if text == "Conversation continues")
        );
    }
    #[test]
    fn upgrading_an_inert_view_never_excuses_tampered_headers_signature_chain_or_supported_body() {
        for change in 0..7 {
            let mut events = predecessor();
            match change {
                0 => events[1].sig = "00".repeat(64),
                1 => events[1].author = "00".repeat(32),
                2 => events[1].prev = "ff".repeat(32),
                3 => {
                    events[1].body = EventBody::Unsupported {
                        kind: "Other".into(),
                    }
                }
                4 => events[1].body = events[2].body.clone(),
                5 => events[1].seen = None,
                _ => events[1].content[0] ^= 1,
            }
            assert!(
                SpaceLog::from_events("sp_target_cohort", events).is_err(),
                "{change}"
            );
        }
    }
    #[test]
    fn recognized_partial_metadata_is_hydrated_then_left_to_normal_application_admission() {
        for removed in ["agent_id", "intent_sha256"] {
            let old = predecessor();
            let mut signed = SignedContent::parse(&old[1].content).unwrap();
            let Some(Payload::Body(bytes)) = signed.payload.take() else {
                panic!("body")
            };
            let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            value["AppEditRequested"]["reply"]["artifact"]
                .as_object_mut()
                .unwrap()
                .remove(removed);
            signed.payload = Some(Payload::Body(serde_json::to_vec(&value).unwrap()));
            let bytes = signed.encode();
            let signature = crate::Signer::from_secret(&[143; 32])
                .sign(content::content_hash(&bytes).as_bytes());
            let mut event = crate::event_from_content(
                bytes.clone(),
                signature,
                None,
                0,
                String::new(),
                String::new(),
            )
            .unwrap();
            let recognized = event.body.clone();
            event.body = EventBody::Unsupported {
                kind: "AppEditRequested".into(),
            };
            let hydrated = hydrate(event);
            assert_eq!(hydrated.body, recognized);
            assert_eq!(hydrated.content, bytes);
            verify_author(&hydrated).unwrap();
        }
    }

    #[test]
    fn a_future_unrecognized_request_stays_inert_and_keeps_its_authentic_bytes() {
        let old = predecessor();
        let mut signed = SignedContent::parse(&old[1].content).unwrap();
        let Some(Payload::Body(bytes)) = signed.payload.take() else {
            panic!("body")
        };
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["AppEditRequested"]["reply"]["artifact"]["future_authority"] = true.into();
        signed.payload = Some(Payload::Body(serde_json::to_vec(&value).unwrap()));
        let bytes = signed.encode();
        let signature =
            crate::Signer::from_secret(&[143; 32]).sign(content::content_hash(&bytes).as_bytes());
        let event = crate::event_from_content(
            bytes.clone(),
            signature,
            None,
            0,
            String::new(),
            String::new(),
        )
        .unwrap();
        assert!(
            matches!(&event.body, EventBody::Unsupported { kind } if kind == "AppEditRequested")
        );
        let hydrated = hydrate(event.clone());
        assert_eq!(hydrated, event);
        assert_eq!(hydrated.content, bytes);
        verify_author(&hydrated).unwrap();
    }
}
