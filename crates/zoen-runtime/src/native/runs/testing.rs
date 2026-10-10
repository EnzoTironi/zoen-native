use super::*;

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
