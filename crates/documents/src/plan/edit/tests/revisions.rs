//! Existing typed editor revisions regressions.
use super::*;

#[test]
fn empty_edits_and_equivalent_span_segmentation_do_not_create_operations_or_drop_metadata() {
    let mut doc = seed().open_editor().unwrap();
    let mut changed = edit(&line_block(&doc));
    changed.spans = vec![span(RAW, "b")];
    apply(&mut doc, &[changed]);
    let revision = doc.revision().unwrap();
    let before = doc.blocks().unwrap();
    assert_eq!(
        doc.apply_from(&revision, &order(&doc), &[]).unwrap(),
        CommandResult::Unchanged
    );
    let mut same = edit(&line_block(&doc));
    same.spans = vec![
        Span {
            start: 0,
            end: 1,
            key: "b".into(),
            value: String::new(),
        },
        Span {
            start: 1,
            end: u32::try_from(RAW.encode_utf16().count()).unwrap(),
            key: "b".into(),
            value: String::new(),
        },
    ];
    assert_eq!(
        doc.apply_from(&revision, &order(&doc), &[same]).unwrap(),
        CommandResult::Unchanged
    );
    assert_unchanged(&doc, &revision, &before);
}

#[test]
fn oversized_typed_edits_refuse_without_mutating_the_prior_revision() {
    let mut doc = seed().open_editor().unwrap();
    let revision = doc.revision().unwrap();
    let before = doc.blocks().unwrap();
    let mut changed = edit(&line_block(&doc));
    changed.text = "x".repeat(MAX_PLAN_PACKET_BYTES + 1);
    assert!(matches!(
        doc.apply_from(&revision, &order(&doc), &[changed]),
        Err(PlanError::Limit(_))
    ));
    assert_unchanged(&doc, &revision, &before);
}

#[test]
fn plan_revision_token_roundtrip_retains_exact_history() {
    let mut doc = seed().open_editor().unwrap();
    let before = doc.revision().unwrap();
    let mut changed = edit(&line_block(&doc));
    changed.text = "token 😀\r\n\0history".into();
    changed.spans = vec![span(&changed.text, "b")];
    doc.apply_from(&before, &order(&doc), &[changed]).unwrap();
    let current = doc.revision().unwrap();
    let history = doc.full_update().to_json().unwrap();
    for revision in [before, current] {
        let encoded = revision.encode().unwrap();
        let verified = doc.verify_revision(&encoded).unwrap();
        assert_eq!(verified, revision);
        assert_eq!(verified.encode().unwrap(), encoded);
        assert_eq!(doc.at(&verified).unwrap().revision().unwrap(), revision);
    }
    assert_eq!(doc.full_update().to_json().unwrap(), history);
}

#[test]
fn plan_revision_token_rejects_unknown_fields_and_every_forged_witness_atomically() {
    let doc = seed().open_editor().unwrap();
    let revision = doc.revision().unwrap();
    let history = doc.full_update().to_json().unwrap();
    let encoded = revision.encode().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    let mut invalid = Vec::new();
    for field in ["schema", "root", "frontiers_hex", "operations_hex"] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        invalid.push(serde_json::to_vec(&missing).unwrap());
        let mut forged = value.clone();
        forged[field] = serde_json::Value::String("forged".into());
        invalid.push(serde_json::to_vec(&forged).unwrap());
    }
    let mut digest = value.clone();
    digest["operations_hex"] = serde_json::Value::String("00".repeat(32));
    invalid.push(serde_json::to_vec(&digest).unwrap());
    let mut unknown = value;
    unknown["version"] = serde_json::Value::String("caller-chosen-vv".into());
    invalid.push(serde_json::to_vec(&unknown).unwrap());
    let duplicate = String::from_utf8(encoded).unwrap();
    invalid.push(format!("{{\"schema\":\"duplicate\",{}", &duplicate[1..]).into_bytes());
    invalid.push(vec![b' '; PlanRevision::MAX_ENCODED_BYTES + 1]);
    for packet in invalid {
        assert!(doc.verify_revision(&packet).is_err());
        assert_eq!(doc.revision().unwrap(), revision);
        assert_eq!(doc.full_update().to_json().unwrap(), history);
    }
}

#[test]
fn plan_revision_token_refuses_cross_history_and_retries_after_dependencies() {
    let mut source = seed().open_editor().unwrap();
    let mut receiver = source.fork().unwrap();
    let base = source.version();
    let mut changed = edit(&line_block(&source));
    changed.text = "unknown causal operations".into();
    apply(&mut source, &[changed]);
    let token = source.revision().unwrap().encode().unwrap();
    let exact = receiver.full_update().to_json().unwrap();
    assert!(receiver.verify_revision(&token).is_err());
    assert_eq!(receiver.full_update().to_json().unwrap(), exact);
    let other_seed = CanonicalPlanSeed::replay(&[LegacyVersion {
        event_id: "another-authenticated-creation".into(),
        document: original(),
    }])
    .unwrap();
    let other = other_seed.open_editor().unwrap();
    assert!(other.verify_revision(&token).is_err());
    receiver
        .import(&source.updates_since(&base).unwrap())
        .unwrap();
    assert_eq!(
        receiver.verify_revision(&token).unwrap(),
        source.revision().unwrap()
    );
    assert_eq!(receiver.blocks().unwrap(), source.blocks().unwrap());
}
