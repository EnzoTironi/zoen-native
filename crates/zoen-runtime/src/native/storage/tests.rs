use super::*;
#[path = "capacity_tests.rs"]
mod capacity;

#[cfg(test)]
pub(in crate::native) async fn test_cuts(
    execution: &Execution,
    custody: &crate::native::NativeCustody,
    principal: &Principal,
    image: &client_core::runtime_core::NativeImage,
) {
    let current = execution.read_native(principal, None).await.unwrap().root;
    let trx = execution.transaction().await.unwrap();
    let mut run_range = foundationdb::RangeOption::from(execution.root.subspace(&("run",)).range());
    run_range.limit = Some(1);
    assert!(
        trx.get_range(&run_range, 1, false)
            .await
            .unwrap()
            .is_empty(),
        "device maintenance has no fabricated run"
    );
    drop(trx);
    let fence = execution.acquire_native_device(principal).await.unwrap();
    assert!(matches!(
        execution.acquire_native_device(principal).await,
        Err(RuntimeError::Denied)
    ));
    let staged = execution
        .stage_native(
            principal,
            Some(current.clone()),
            &fence,
            custody
                .seal_image(principal, Some(&current), image)
                .unwrap(),
        )
        .await
        .unwrap();
    let generation = execution.native_generation(&staged.root);
    execution.collect_native_stage(principal).await.unwrap();
    let trx = execution.transaction().await.unwrap();
    assert!(
        trx.get(&generation.pack(&("manifest",)), false)
            .await
            .unwrap()
            .is_some(),
        "live pin protects staging"
    );
    assert_eq!(
        execution.native_root(&trx, principal).await.unwrap(),
        Some(current.clone())
    );
    let values = bounded_rows(
        &trx,
        foundationdb::RangeOption::from(generation.range()),
        MAX_CHUNKS + 1,
        CHUNK_BYTES + 40,
    )
    .await
    .unwrap();
    assert_eq!(values.len(), staged.root.context.chunks as usize + 1);
    for (_, bytes) in values {
        assert!(!bytes.windows(15).any(|w| w == b"SQLite format 3"));
        assert!(!bytes.windows(32).any(|w| w == [73; 32]));
    }
    drop(trx);
    let pending = execution.transaction().await.unwrap();
    execution
        .native_activation(&pending, &staged, &fence)
        .await
        .unwrap();
    let concurrent = execution.transaction().await.unwrap();
    concurrent.set(
        &execution.native_key(principal, "root"),
        &encode(&current).unwrap(),
    );
    concurrent.commit().await.unwrap();
    assert!(
        pending.commit().await.is_err(),
        "a concurrent root write conflicts with activation"
    );
    assert_eq!(
        execution.read_native(principal, None).await.unwrap().root,
        current
    );
    execution
        .release_native_device(principal, &fence)
        .await
        .unwrap();
    let takeover = execution.acquire_native_device(principal).await.unwrap();
    assert!(takeover.token > fence.token);
    let check = execution.transaction().await.unwrap();
    assert!(matches!(
        execution.native_activation(&check, &staged, &fence).await,
        Err(RuntimeError::Denied)
    ));
    assert!(
        matches!(
            execution
                .native_activation(&check, &staged, &takeover)
                .await,
            Err(RuntimeError::Denied)
        ),
        "new holder cannot adopt an old holder's staged workspace"
    );
    assert_eq!(
        execution
            .release_native_device(principal, &fence)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    let trx = execution.transaction().await.unwrap();
    let stage_key = execution.native_key(principal, "stage");
    let mut expired: Stage = decode(&trx.get(&stage_key, false).await.unwrap().unwrap()).unwrap();
    // Explicit version-clock fixture cut; no elapsed retention or wire-level
    // commit-unknown fault is claimed by this setup.
    expired.expires = trx.get_read_version().await.unwrap() - ORPHAN_RETENTION_VERSIONS - 1;
    trx.set(&stage_key, &encode(&expired).unwrap());
    trx.commit().await.unwrap();
    execution.collect_native_stage(principal).await.unwrap();
    let trx = execution.transaction().await.unwrap();
    assert!(trx
        .get(&generation.pack(&("manifest",)), false)
        .await
        .unwrap()
        .is_none());
    assert!(trx.get(&stage_key, false).await.unwrap().is_none());
    assert_eq!(
        execution.native_root(&trx, principal).await.unwrap(),
        Some(current.clone())
    );
    assert!(
        trx.get(
            &execution.native_generation(&current).pack(&("manifest",)),
            false
        )
        .await
        .unwrap()
        .is_some(),
        "GC preserves active image"
    );
    drop(trx);
    let next = execution
        .stage_native(
            principal,
            Some(current.clone()),
            &takeover,
            custody
                .seal_image(principal, Some(&current), image)
                .unwrap(),
        )
        .await
        .unwrap();
    let expected = next.root.clone();
    let admission = crate::finance::NativeActivation::fixture(next.scope(execution, &takeover));
    let activated = execution
        .activate_native(next, &takeover, admission)
        .await
        .unwrap();
    // Suppress a known committed activation reply and reload the durable root;
    // there is no recreated live dispatch capability.
    drop(activated);
    let restored = execution
        .read_native(principal, Some(&takeover))
        .await
        .unwrap();
    assert_eq!(restored.root, expected);
    assert_eq!(
        custody.open_image(principal, restored).unwrap().as_bytes(),
        image.as_bytes()
    );
    execution.collect_native_retired(principal).await.unwrap();
    let previous_key = execution
        .native_retired(principal)
        .pack(&(current.context.capsule.as_str(),));
    let trx = execution.transaction().await.unwrap();
    let mut retired: Retired =
        decode(&trx.get(&previous_key, false).await.unwrap().unwrap()).unwrap();
    assert!(
        trx.get(
            &execution.native_generation(&current).pack(&("manifest",)),
            false
        )
        .await
        .unwrap()
        .is_some(),
        "retention protects the previous image"
    );
    retired.collect_after = trx.get_read_version().await.unwrap() - 1;
    trx.set(&previous_key, &encode(&retired).unwrap());
    let reference = execution
        .native_references(&current)
        .pack(&("pending-original-attempt",));
    // Explicit retained-reference fixture, not a production run or admission.
    trx.set(&reference, b"1");
    trx.commit().await.unwrap();
    execution.collect_native_retired(principal).await.unwrap();
    let trx = execution.transaction().await.unwrap();
    assert!(
        trx.get(&previous_key, false).await.unwrap().is_some(),
        "a retained reference prevents collection"
    );
    trx.clear(&reference);
    trx.commit().await.unwrap();
    execution.collect_native_retired(principal).await.unwrap();
    let trx = execution.transaction().await.unwrap();
    assert!(trx.get(&previous_key, false).await.unwrap().is_none());
    assert!(trx
        .get(
            &execution.native_generation(&current).pack(&("manifest",)),
            false
        )
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        execution.native_root(&trx, principal).await.unwrap(),
        Some(expected.clone())
    );
    let active_retirement_key = execution
        .native_retired(principal)
        .pack(&(expected.context.capsule.as_str(),));
    let mut corrupt_metadata = expected.clone();
    corrupt_metadata.manifest_digest = hash(b"corrupt retirement metadata");
    trx.set(
        &active_retirement_key,
        &encode(&Retired {
            root: corrupt_metadata,
            collect_after: 0,
        })
        .unwrap(),
    );
    trx.commit().await.unwrap();
    execution.collect_native_retired(principal).await.unwrap();
    let trx = execution.transaction().await.unwrap();
    assert!(
        trx.get(
            &execution.native_generation(&expected).pack(&("manifest",)),
            false
        )
        .await
        .unwrap()
        .is_some(),
        "physical active capsule remains protected even with a mismatching retirement digest"
    );
    trx.clear(&active_retirement_key);
    trx.commit().await.unwrap();
    execution
        .release_native_device(principal, &takeover)
        .await
        .unwrap();
    capacity::run(execution, custody, principal, image)
        .await
        .expect("bounded native retirement must recover without an orphan stage");
}
