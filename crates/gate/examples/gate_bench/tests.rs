use super::{auroc, percentile, providers, Case};

#[test]
fn latency_rank_preserves_rounding_and_empty_samples() {
    assert_eq!(percentile(&[], 50), None);
    assert_eq!(percentile(&[19], 95), Some(19));
    assert_eq!(percentile(&[3, 7], 50), Some(7));
    let samples: Vec<u64> = (0..201).collect();
    assert_eq!(percentile(&samples, 50), Some(100));
    assert_eq!(percentile(&samples, 95), Some(190));
    assert_eq!(percentile(&samples, 0), Some(0));
    assert_eq!(percentile(&samples, 100), Some(200));
    assert_eq!(percentile(&samples, 101), None);
}

#[test]
fn auroc_keeps_exact_ties_and_refuses_undefined_comparisons() {
    assert_eq!(auroc(&[(0.9, true), (0.1, false)]), Some(1.0));
    assert_eq!(auroc(&[(0.1, true), (0.9, false)]), Some(0.0));
    assert_eq!(auroc(&[(0.5, true), (0.5, false)]), Some(0.5));
    assert_eq!(auroc(&[(-0.0, true), (0.0, false)]), Some(0.5));
    assert_eq!(auroc(&[(0.5, true)]), None);
    assert_eq!(auroc(&[(f64::NAN, true), (0.5, false)]), None);
}

#[test]
fn invalid_provider_and_missing_credentials_are_errors_not_panics() {
    assert_eq!(
        providers("missing-separator", |_| None).err().as_deref(),
        Some("GATE_PROVIDERS requires name=url#model")
    );
    assert_eq!(
        providers("clef=cloudflare", |_| None).err().as_deref(),
        Some("CLOUDFLARE_ACCOUNT_ID is not set or not Unicode")
    );
    assert_eq!(
        providers("clef=cloudflare", |key| (key == "CLOUDFLARE_ACCOUNT_ID")
            .then(|| "isolated-fixture".into()))
        .err()
        .as_deref(),
        Some("CLOUDFLARE_API_TOKEN is not set or not Unicode")
    );
    let local = providers("", |_| None).expect("mock needs no environment or network");
    assert_eq!(local.len(), 1);
    assert_eq!(local[0].0, "mock");
}

#[test]
fn both_embedded_case_sets_parse_before_any_provider_runs() {
    for raw in [
        include_str!("../../fixtures/redteam.json"),
        include_str!("../../fixtures/heldout.json"),
    ] {
        let cases: Vec<Case> = serde_json::from_str(raw).expect("embedded benchmark fixtures");
        assert!(!cases.is_empty());
    }
}
