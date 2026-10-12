use super::*;

pub(crate) async fn repack_holder(runtime: &RuntimeAuthority, step: &crate::VerifiedStep) {
    let crate::VerifiedStep::Native(step) = step else {
        panic!("actual native holder required");
    };
    let principal = &step.loaded.record.original.context.principal;
    let custody = runtime.native.as_ref().unwrap();
    let credential = custody
        .credential(&principal.agent, &principal.device)
        .unwrap();
    custody
        .repack_fenced(runtime, credential, principal, &step.loaded.fences.device)
        .await
        .unwrap();
}

pub(crate) async fn lease_snapshot(
    runtime: &RuntimeAuthority,
    run: &str,
) -> (i64, Vec<(String, i64, i64)>) {
    let (record, _) = runtime
        .execution
        .read_reply(run, &runtime.custody)
        .await
        .unwrap();
    let trx = runtime.execution.transaction().await.unwrap();
    let version = trx.get_read_version().await.unwrap();
    let mut leases = Vec::new();
    for key in [
        runtime.execution.run(run, "lease"),
        runtime
            .execution
            .native_lease(&record.original.context.principal),
    ] {
        let bytes = trx.get(&key, false).await.unwrap().unwrap();
        let lease: crate::execution::Lease = serde_json::from_slice(&bytes).unwrap();
        leases.push((lease.holder, lease.token, lease.expires));
    }
    (version, leases)
}

pub(crate) async fn original(runtime: &RuntimeAuthority, run: &str) -> (String, String) {
    let (record, digest) = runtime
        .execution
        .read_reply(run, &runtime.custody)
        .await
        .unwrap();
    let [InputMessage::User { text }] = record.request.messages.as_slice() else {
        panic!()
    };
    (text.clone(), digest)
}

pub(crate) async fn stale_admission(runtime: &RuntimeAuthority, step: &crate::VerifiedStep) {
    let crate::VerifiedStep::Native(native) = step else {
        panic!("actual native step required");
    };
    let loaded = &native.loaded;
    let principal = &loaded.record.original.context.principal;
    let trx = runtime.execution.transaction().await.unwrap();
    let version = trx.get_read_version().await.unwrap();
    // Explicit fixture expiry cut; the separate cancellation journey waits for
    // real expiry without changing the 60-million-version production deadline.
    for key in [
        runtime.execution.run(&loaded.record.locator.run, "lease"),
        runtime.execution.native_lease(principal),
    ] {
        let bytes = trx.get(&key, false).await.unwrap().unwrap();
        let mut lease: crate::execution::Lease = serde_json::from_slice(&bytes).unwrap();
        lease.expires = version;
        trx.set(&key, &serde_json::to_vec(&lease).unwrap());
    }
    trx.commit().await.unwrap();
    let successor = runtime
        .execution
        .acquire_reply(&loaded.record, &loaded.record_digest)
        .await
        .unwrap();
    assert_eq!(successor.run.token, loaded.fences.run.token + 1);
    assert_eq!(successor.device.token, loaded.fences.device.token + 1);
    let binding = &loaded.record.binding;
    runtime.finance.reserve(binding).await.unwrap();
    let claim = runtime.finance.claim(binding).await.unwrap().unwrap();
    let guard = runtime.finance.guard(binding, step).await.unwrap();
    assert!(matches!(
        runtime
            .execution
            .admit_once(step, claim, binding, &runtime.custody, runtime)
            .await,
        Err(RuntimeError::Denied)
    ));
    guard.rollback().await.unwrap();
    assert_eq!(
        native.release(&runtime.execution).await.unwrap_err(),
        RuntimeError::Denied
    );
    runtime
        .execution
        .release_reply(&loaded.record, &successor)
        .await
        .unwrap();
}

pub(crate) async fn fences(runtime: &RuntimeAuthority, run: &str) {
    let custody = runtime.native.as_ref().unwrap();
    let loaded = custody.load_reply(runtime, run).await.unwrap();
    let (_, digest) = runtime
        .execution
        .read_reply(run, &runtime.custody)
        .await
        .unwrap();
    let record = &loaded.record;
    let principal = &record.original.context.principal;
    assert!(matches!(
        runtime.execution.acquire_reply(record, &digest).await,
        Err(RuntimeError::Denied)
    ));
    let trx = runtime.execution.transaction().await.unwrap();
    let version = trx.get_read_version().await.unwrap();
    for key in [
        runtime.execution.run(run, "lease"),
        runtime.execution.native_lease(principal),
    ] {
        let bytes = trx.get(&key, false).await.unwrap().unwrap();
        let mut lease: crate::execution::Lease = serde_json::from_slice(&bytes).unwrap();
        lease.expires = version;
        trx.set(&key, &serde_json::to_vec(&lease).unwrap());
    }
    trx.commit().await.unwrap();
    let fences = runtime
        .execution
        .acquire_reply(record, &digest)
        .await
        .unwrap();
    assert_eq!(fences.run.token, loaded.fences.run.token + 1);
    assert_eq!(fences.device.token, loaded.fences.device.token + 1);
    let current = runtime
        .execution
        .read_native(principal, Some(&fences.device))
        .await
        .unwrap()
        .root;
    assert_eq!(
        runtime
            .execution
            .verify_reply(
                record,
                &digest,
                &current,
                &loaded.facts,
                &loaded.fences,
                &runtime.custody
            )
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(
        runtime
            .execution
            .release_reply(record, &loaded.fences)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    runtime
        .execution
        .verify_reply(
            record,
            &digest,
            &current,
            &loaded.facts,
            &fences,
            &runtime.custody,
        )
        .await
        .unwrap();
    runtime
        .execution
        .release_reply(record, &fences)
        .await
        .unwrap();
    println!("retained reply fence cut: actual run/device takeover rejects stale snapshot and release PASS");

    let trx = runtime.execution.transaction().await.unwrap();
    let key = runtime.execution.root.pack(&(
        "native-retired",
        principal.agent.as_str(),
        principal.device.as_str(),
        record.original.context.capsule.as_str(),
    ));
    let bytes = trx.get(&key, false).await.unwrap().unwrap();
    let mut retired: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    retired["collect_after"] = serde_json::json!(trx.get_read_version().await.unwrap());
    trx.set(&key, &serde_json::to_vec(&retired).unwrap());
    trx.commit().await.unwrap();
    runtime
        .execution
        .collect_native_retired(principal)
        .await
        .unwrap();
    let trx = runtime.execution.transaction().await.unwrap();
    assert!(trx.get(&key, false).await.unwrap().is_some());
    let manifest = runtime.execution.root.pack(&(
        "native-image",
        principal.agent.as_str(),
        principal.device.as_str(),
        record.original.context.capsule.as_str(),
        "manifest",
    ));
    assert!(trx.get(&manifest, false).await.unwrap().is_some());
    println!("retained reply GC cut: original run reference protects retired native capsule PASS");
}
