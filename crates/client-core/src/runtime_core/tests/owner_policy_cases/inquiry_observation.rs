//! Observations share actual signed-MLS discovery and cannot create a receipt.
use super::*;

#[test]
fn inquiry_scan_distinguishes_eligible_rejected_and_reported_without_publishing() -> TestResult {
    let (mut f, scope, _) = setup()?;
    let pending = f.owner.request_account_policy_options(&scope)?;
    deliver_owner(&mut f)?;
    let before = f.core.image()?;
    let scan = f
        .core
        .engine
        .scan_owner_policy_inquiries(&scope.space_id, 0, 50)?;
    assert_eq!(scan.counts.candidates, 1);
    assert_eq!(scan.counts.rejected, 0);
    assert_eq!(scan.counts.already_reported, 0);
    assert_eq!(scan.inquiries.len(), 1);
    assert_eq!(scan.inquiries[0].source(), pending.source);
    assert_eq!(f.core.image()?.as_bytes(), before.as_bytes());
    // A present owner whose write access has changed is a rejected candidate,
    // not missing delivery and never an implicit Runtime refusal.
    let role = &mut f
        .core
        .engine
        .state
        .spaces
        .get_mut(&scope.space_id)
        .ok_or("missing private DM")?
        .members
        .iter_mut()
        .find(|(id, _)| id == &scope.expected_identity_id)
        .ok_or("missing owner")?
        .1;
    let prior = *role;
    *role = types::Role::Reader;
    let rejected = f
        .core
        .engine
        .scan_owner_policy_inquiries(&scope.space_id, 0, 50)?;
    assert_eq!(rejected.counts.candidates, 1);
    assert_eq!(rejected.counts.rejected, 1);
    assert!(rejected.inquiries.is_empty());
    f.core
        .engine
        .state
        .spaces
        .get_mut(&scope.space_id)
        .ok_or("missing DM")?
        .members
        .iter_mut()
        .find(|(id, _)| id == &scope.expected_identity_id)
        .ok_or("missing owner")?
        .1 = prior;
    assert_eq!(f.core.image()?.as_bytes(), before.as_bytes());
    let query = f
        .core
        .engine
        .owner_policy_inquiry(&scope.space_id, &pending.source)?;
    f.core.engine.refuse_owner_policy_options(&query)?;
    let reported = f
        .core
        .engine
        .scan_owner_policy_inquiries(&scope.space_id, 0, 50)?;
    assert_eq!(reported.counts.candidates, 1);
    assert_eq!(reported.counts.already_reported, 1);
    assert_eq!(reported.counts.rejected, 0);
    assert!(reported.inquiries.is_empty());
    assert!(f
        .core
        .engine
        .scan_owner_policy_inquiries(&scope.space_id, 0, 0)
        .is_err());
    let after = f.core.image()?;
    let repeated = f
        .core
        .engine
        .scan_owner_policy_inquiries(&scope.space_id, 0, 50)?;
    assert_eq!(repeated.counts, reported.counts);
    assert!(repeated.inquiries.is_empty());
    assert_eq!(after.as_bytes(), f.core.image()?.as_bytes());
    Ok(())
}
