//! Actual SQL read-only account context, without an image Request or provider.
use super::*;
use types::owner_policy_context::{OwnerPolicyInquiry, OwnerPolicyInquiryPurpose};

pub(super) async fn run(w: &World) {
    let now = w.runtime.finance.reply_clock().await.unwrap();
    let inquiry = OwnerPolicyInquiry {
        id: "account_management_query".into(),
        owner: w.owner.identity.clone(),
        agent: w.agents[0].identity.clone(),
        space: "owner_private_control".into(),
        request: String::new(),
        content_hash: String::new(),
        price_digest: String::new(),
        purpose: Some(OwnerPolicyInquiryPurpose::AccountBudget),
        at_ms: now,
    };
    let signed = event_log::owner_policy_inquiry::sign(&w.owner, inquiry).unwrap();
    let price = image_tasks::profile();
    let before = w.count("SELECT count(*) FROM runtime_policies").await;
    let options = owner_policy_controls::guard(w, &w.runtime)
        .await
        .unwrap()
        .owner_policy_options(&signed, &price)
        .await
        .unwrap();
    assert_eq!(options.image_profile_digest, price.digest().unwrap());
    assert_eq!(options.reservation_units, price.reservation_units);
    let spec = options.image_spec.unwrap();
    assert_eq!(spec.size, "1024x1024");
    assert_eq!(spec.quality, "low");
    assert_eq!(
        options.current_policy_digest.as_ref(),
        Some(&w.policy_digest)
    );
    assert_eq!(options.current_policy.unwrap().policy, w.signed.policy);
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_policies").await,
        before
    );
    for table in [
        "runtime_attempts",
        "runtime_claims",
        "runtime_journals",
        "runtime_postings",
    ] {
        assert_eq!(w.count(&format!("SELECT count(*) FROM {table}")).await, 0);
    }
    assert_eq!(w.http.sends.load(Ordering::SeqCst), 0);
    let mut stale = signed.inquiry.clone();
    stale.at_ms = now - types::owner_policy_context::CONTEXT_TTL_MS - 1;
    let stale = event_log::owner_policy_inquiry::sign(&w.owner, stale).unwrap();
    assert_eq!(
        owner_policy_controls::guard(w, &w.runtime)
            .await
            .unwrap()
            .owner_policy_options(&stale, &price)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    println!("account policy context: actual directory/device/person/SQL period/latest signed row +configured image quote without Request, zero financial/provider effects PASS");
}
