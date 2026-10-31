use super::super::admission_regressions::{dispatch, dispatch_for};
use super::*;
use std::sync::{Arc, Barrier};
use std::time::Duration;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
fn day(n: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(n * 86_400)
}

#[test]
fn quota_is_per_owner_and_utc_day_and_never_forgets_an_admitted_attempt() -> TestResult {
    let quota = CheapTierAuthority::new(1);
    let one = dispatch_for("one", "owner")?;
    let two = dispatch_for("two", "owner")?;
    let other = dispatch_for("other", "another")?;
    assert_eq!(quota.admit_at(&one, day(0)), Ok(Admission::Fresh));
    assert_eq!(quota.admit_at(&two, day(0)), Err(AdmissionError::Denied));
    assert_eq!(quota.admit_at(&two, day(0)), Err(AdmissionError::Denied));
    assert_eq!(quota.admit_at(&other, day(0)), Ok(Admission::Fresh));
    assert_eq!(quota.admit_at(&one, day(1)), Ok(Admission::AlreadyKnown));
    assert_eq!(quota.admit_at(&two, day(1)), Ok(Admission::Fresh));
    assert_eq!(quota.admit_at(&two, day(2)), Ok(Admission::AlreadyKnown));
    Ok(())
}

#[test]
fn every_dispatch_field_is_bound_and_conflicts_cannot_replace_the_original() -> TestResult {
    let quota = CheapTierAuthority::new(1);
    let request = dispatch()?;
    assert_eq!(quota.admit_at(&request, day(0)), Ok(Admission::Fresh));
    let mutations: [fn(&mut DispatchRequest); 18] = [
        |r| r.context.run_id.push('x'),
        |r| r.context.owner.push('x'),
        |r| r.context.agent.push('x'),
        |r| r.context.device.push('x'),
        |r| r.context.space.push('x'),
        |r| r.context.authority_version.push('x'),
        |r| r.context.definition_version.push('x'),
        |r| r.context.price_version.push('x'),
        |r| r.request_digest.push('x'),
        |r| r.profile = "another-profile",
        |r| r.descriptor.operation = Operation::Embedding,
        |r| r.descriptor.endpoint.push('x'),
        |r| r.descriptor.model.push('x'),
        |r| r.descriptor.credential_ref.push('x'),
        |r| r.descriptor.encoded_request_bytes += 1,
        |r| r.descriptor.max_request_bytes += 1,
        |r| r.descriptor.max_response_bytes += 1,
        |r| r.descriptor.requested_output_tokens += 1,
    ];
    for change in mutations {
        let mut changed = request.clone();
        change(&mut changed);
        assert_eq!(
            quota.admit_at(&changed, day(0)),
            Err(AdmissionError::Denied)
        );
        assert_eq!(
            quota.admit_at(&request, day(0)),
            Ok(Admission::AlreadyKnown)
        );
    }
    Ok(())
}

#[test]
fn concurrent_new_attempts_cannot_overdraw_the_shared_quota() -> TestResult {
    let quota = Arc::new(CheapTierAuthority::new(7));
    let requests = (0..24)
        .map(|i| dispatch_for(&format!("attempt_{i}"), "owner"))
        .collect::<TestResult<Vec<_>>>()?;
    let barrier = Arc::new(Barrier::new(requests.len()));
    let mut handles = Vec::new();
    for request in requests {
        let quota = quota.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            quota.admit_at(&request, day(0))
        }));
    }
    let mut fresh = 0;
    let mut denied = 0;
    for handle in handles {
        match handle.join().map_err(|_| "quota fixture thread panicked")? {
            Ok(Admission::Fresh) => fresh += 1,
            Err(AdmissionError::Denied) => denied += 1,
            _ => return Err("unexpected admission result".into()),
        }
    }
    assert_eq!((fresh, denied), (7, 17));
    Ok(())
}

#[test]
fn concurrent_copies_of_the_same_attempt_create_one_fresh_admission() -> TestResult {
    let quota = Arc::new(CheapTierAuthority::new(1));
    let request = dispatch()?;
    let barrier = Arc::new(Barrier::new(24));
    let mut handles = Vec::new();
    for _ in 0..24 {
        let quota = quota.clone();
        let request = request.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            quota.admit_at(&request, day(0))
        }));
    }
    let mut fresh = 0;
    let mut known = 0;
    for handle in handles {
        match handle
            .join()
            .map_err(|_| "quota fixture thread panicked")??
        {
            Admission::Fresh => fresh += 1,
            Admission::AlreadyKnown => known += 1,
        }
    }
    assert_eq!((fresh, known), (1, 23));
    assert_eq!(
        quota.admit_at(&dispatch_for("next", "owner")?, day(0)),
        Err(AdmissionError::Denied)
    );
    Ok(())
}

#[tokio::test]
async fn invalid_clock_and_poisoned_state_fail_closed_without_a_panic() -> TestResult {
    let quota = CheapTierAuthority::new(1);
    let request = dispatch()?;
    assert_eq!(
        quota.admit_at(&request, UNIX_EPOCH - Duration::from_secs(1)),
        Err(AdmissionError::Unavailable)
    );
    assert_eq!(quota.admit_at(&request, day(0)), Ok(Admission::Fresh));
    assert_eq!(
        quota.admit_at(&request, UNIX_EPOCH - Duration::from_secs(1)),
        Ok(Admission::AlreadyKnown)
    );
    let poisoned = Arc::new(CheapTierAuthority::new(1));
    let other = poisoned.clone();
    assert!(std::thread::spawn(move || {
        let Ok(_held) = other.state.lock() else {
            return;
        };
        panic!("synthetic in-flight state failure");
    })
    .join()
    .is_err());
    assert_eq!(
        poisoned.admit(&request).await,
        Err(AdmissionError::Unavailable)
    );
    Ok(())
}

#[test]
fn maximum_counter_admits_its_last_unit_without_overflow_or_repeat_consumption() -> TestResult {
    let quota = CheapTierAuthority::new(u32::MAX);
    let request = dispatch()?;
    quota
        .state
        .lock()
        .map_err(|_| "fixture poisoned")?
        .used
        .insert(("owner".into(), 0), u32::MAX - 1);
    assert_eq!(quota.admit_at(&request, day(0)), Ok(Admission::Fresh));
    assert_eq!(
        quota.admit_at(&request, day(0)),
        Ok(Admission::AlreadyKnown)
    );
    assert_eq!(
        quota.admit_at(&dispatch_for("next", "owner")?, day(0)),
        Err(AdmissionError::Denied)
    );
    Ok(())
}
