//! Actual FDB capsule saturation, retry and normal retirement collection.
use super::*;

fn duplicate(candidate: &PreparedImage) -> PreparedImage {
    PreparedImage {
        root: candidate.root.clone(),
        manifest: candidate.manifest.clone(),
        chunks: candidate.chunks.clone(),
    }
}

pub(super) async fn run(
    parent: &Execution,
    custody: &crate::native::NativeCustody,
    principal: &Principal,
    image: &client_core::runtime_core::NativeImage,
) -> Result<(), RuntimeError> {
    // Keep this storage seam independent of the caller's actual enrolled head,
    // original-task pins and future lifecycle tests. No extra service is made.
    let root = parent.root.subspace(&("retirement-capacity-fixture",));
    Execution::create_deployment(&parent.db, &root, &parent.deployment).await?;
    let execution = Execution::new(parent.db.clone(), root, parent.deployment.clone(), None);
    let fence = execution.acquire_native_device(principal).await?;
    let mut current = None;
    for _ in 0..=MAX_RETIRED {
        let candidate = custody.seal_image(principal, current.as_ref(), image)?;
        let staged = execution
            .stage_native(principal, current, &fence, candidate)
            .await?;
        let admission = crate::finance::NativeActivation::fixture(staged.scope(&execution, &fence));
        current = Some(execution.activate_native(staged, &fence, admission).await?);
    }
    let current = current.ok_or(RuntimeError::InvalidBinding)?;
    let trx = execution.transaction().await?;
    let rows = bounded_rows(
        &trx,
        foundationdb::RangeOption::from(execution.native_retired(principal).range()),
        MAX_RETIRED,
        8192,
    )
    .await?;
    assert_eq!(rows.len(), MAX_RETIRED);
    drop(trx);
    let candidate = custody.seal_image(principal, Some(&current), image)?;
    let candidate_range = execution.native_generation(&candidate.root).range();
    for _ in 0..2 {
        let result = execution
            .stage_native(
                principal,
                Some(current.clone()),
                &fence,
                duplicate(&candidate),
            )
            .await;
        assert!(
            matches!(result, Err(RuntimeError::EvidenceTooLarge)),
            "full retired capacity must reject before persisting a candidate"
        );
        let trx = execution.transaction().await?;
        assert_eq!(
            execution.native_root(&trx, principal).await?,
            Some(current.clone())
        );
        assert!(trx
            .get(&execution.native_key(principal, "stage"), false)
            .await?
            .is_none());
        assert!(
            trx.get_range(
                &foundationdb::RangeOption::from(candidate_range.clone()),
                1,
                false
            )
            .await?
            .is_empty(),
            "capacity backpressure must not leave chunks or a manifest"
        );
    }
    let (key, bytes) = rows.first().ok_or(RuntimeError::InvalidBinding)?;
    let mut retired: Retired = decode(bytes)?;
    let trx = execution.transaction().await?;
    // Explicit fixture version-clock cut. Production retention and all active,
    // staged and original-task reference guards remain unchanged.
    retired.collect_after = trx.get_read_version().await? - 1;
    trx.set(key, &encode(&retired)?);
    trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
    execution.collect_native_retired(principal).await?;
    let trx = execution.transaction().await?;
    assert!(trx.get(key, false).await?.is_none());
    assert_eq!(
        execution.native_root(&trx, principal).await?,
        Some(current.clone())
    );
    drop(trx);

    // Exact sealed candidate and still-current fence, without clearing a stage
    // or invoking orphan recovery. A refused attempt had no state to adopt.
    let expected = candidate.root.clone();
    let staged = execution
        .stage_native(principal, Some(current), &fence, candidate)
        .await?;
    let admission = crate::finance::NativeActivation::fixture(staged.scope(&execution, &fence));
    assert_eq!(
        execution.activate_native(staged, &fence, admission).await?,
        expected
    );
    let restored = execution.read_native(principal, Some(&fence)).await?;
    assert_eq!(restored.root, expected);
    assert_eq!(
        custody.open_image(principal, restored)?.as_bytes(),
        image.as_bytes()
    );
    let trx = execution.transaction().await?;
    assert_eq!(
        bounded_rows(
            &trx,
            foundationdb::RangeOption::from(execution.native_retired(principal).range()),
            MAX_RETIRED,
            8192
        )
        .await?
        .len(),
        MAX_RETIRED
    );
    drop(trx);
    execution.release_native_device(principal, &fence).await?;
    let trx = execution.transaction().await?;
    let (begin, end) = execution.root.range();
    trx.clear_range(&begin, &end);
    trx.commit().await.map_err(|_| RuntimeError::Unavailable)?;
    println!("native retirement capacity: sixteen actual retired capsules -> repeated no-write backpressure -> normal GC -> same candidate/current fence activates PASS");
    Ok(())
}
