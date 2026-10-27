//! schema cases scenarios; existing shared fixture remains in the parent.
use super::*;

#[test]
pub(super) fn strict_shared_schema_and_metadata_refuse_substitution() {
    assert!(UserDefinition::parse_bootstrap(BASE).is_ok());
    let accepted = UserDefinition::parse(BASE, 1)
        .unwrap()
        .edited(edit().persona, edit().rules)
        .unwrap();
    assert!(UserDefinition::parse_bootstrap(&accepted).is_err());
    assert!(UserDefinition::parse(&accepted, 2).is_ok());
    for source in [
        BASE.replace("\"kind\":\"user\"", "\"kind\":\"creator\""),
        BASE.replace("\"cheap\"", "\"premium\""),
        BASE.replace("\"state\":{}", "\"state\":{\"token\":\"no\"}"),
        BASE.replace("\"version\":1", "\"version\":1,\"version\":1"),
        BASE.replace("\"state\":{},", ""),
        BASE.replace("\"kind\":", "\"unknown\":0,\"kind\":"),
        format!(" {BASE}"),
        BASE.replace("\"version\":1", "\"version\":0"),
    ] {
        assert!(UserDefinition::parse(&source, 1).is_err());
    }
    let mut value = record(1, BASE);
    value.version = 2;
    assert!(value.dto(&scope()).is_err());
    value.version = 1;
    value.kind = "Creator".into();
    assert!(value.dto(&scope()).is_err());
    assert!(UserDefinition::parse(BASE, 0).is_err());
    assert!(UserDefinition::parse(BASE, u32::MAX).is_err());
    let max = BASE.replacen("\"version\":1", &format!("\"version\":{}", i32::MAX), 1);
    assert!(UserDefinition::parse(&max, i32::MAX as u32)
        .unwrap()
        .edited(edit().persona, edit().rules)
        .is_err());
    assert!(Record::from_relay(AgentStateRecord {
        status: "active".into(),
        kind: "User".into(),
        definition_version: 1,
        definition: vec![],
        cursors: vec![]
    })
    .is_err());
}

#[test]
pub(super) fn review_digest_binds_full_scope_before_after_and_status() {
    let receipt = Receipt::prepare(scope(), record(1, BASE), edit()).unwrap();
    let review = receipt.review().unwrap();
    assert_eq!(review.proposed.version, 2);
    assert_ne!(
        review.current.definition_sha256,
        review.proposed.definition_sha256
    );
    for field in [
        "owner",
        "device",
        "certificate_sha256",
        "relay",
        "agent",
        "configured_relay",
    ] {
        let mut encoded = serde_json::to_value(&receipt).unwrap();
        encoded["scope"][field] = "changed".into();
        assert!(serde_json::from_value::<Receipt>(encoded)
            .unwrap()
            .review()
            .is_err());
    }
    let mut paused = receipt.clone();
    paused.before.status = "paused".into();
    assert!(paused.review().is_err());
    let mut wrong = edit();
    wrong.sha256 = "0".repeat(64);
    assert!(Receipt::prepare(scope(), record(1, BASE), wrong).is_err());
}

#[test]
pub(super) fn old_base_is_nonterminal_unknown_and_exact_next_bytes_are_required() {
    let receipt = Receipt::prepare(scope(), record(1, BASE), edit()).unwrap();
    assert_eq!(
        receipt
            .outcome(Some(receipt.before.clone()))
            .unwrap()
            .outcome,
        AgentDefinitionUpdateOutcome::Unknown
    );
    assert_eq!(
        receipt.outcome(None).unwrap().outcome,
        AgentDefinitionUpdateOutcome::Unknown
    );
    assert_eq!(
        receipt
            .outcome(Some(receipt.after.clone()))
            .unwrap()
            .outcome,
        AgentDefinitionUpdateOutcome::Confirmed
    );
    let mut different = receipt.after.clone();
    different.bytes = different.bytes.replace("Help with", "Avoid");
    assert_eq!(
        receipt.outcome(Some(different)).unwrap().outcome,
        AgentDefinitionUpdateOutcome::Conflict
    );
    let mut newer = receipt.after.clone();
    newer.version = 3;
    assert_eq!(
        receipt.outcome(Some(newer)).unwrap().outcome,
        AgentDefinitionUpdateOutcome::Conflict
    );
    let mut revoked = receipt.after.clone();
    revoked.status = "revoked".into();
    assert_eq!(
        receipt.outcome(Some(revoked)).unwrap().outcome,
        AgentDefinitionUpdateOutcome::Conflict
    );
}
