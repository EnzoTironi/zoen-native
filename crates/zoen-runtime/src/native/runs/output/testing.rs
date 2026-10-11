use super::*;

pub(crate) async fn provision(runtime: &RuntimeAuthority) -> FreshOutputNamespace {
    let execution = &runtime.execution;
    let trx = execution.transaction().await.unwrap();
    let mut range = foundationdb::RangeOption::from(execution.root.range());
    range.limit = Some(3);
    let rows = trx.get_range(&range, 1, false).await.unwrap();
    let deployment = execution.root.pack(&("deployment",));
    let source = execution.root.pack(&("relay-source/1",));
    assert!(
        !rows.more()
            && rows.len() == 2
            && rows
                .iter()
                .all(|row| row.key() == deployment.as_slice() || row.key() == source.as_slice()),
        "only actual empty isolated namespace may provision fixture output"
    );
    trx.commit().await.unwrap();
    FreshOutputNamespace {
        namespace: execution.root.bytes().to_vec(),
        deployment: execution.deployment.clone(),
        source: execution.reply_source().unwrap().cell().into(),
        observed: Default::default(),
        output_expiry_remaining: Default::default(),
    }
}

pub(crate) fn expiry_cut_remaining(runtime: &RuntimeAuthority) -> u64 {
    runtime
        .output_fixture
        .as_ref()
        .unwrap()
        .output_expiry_remaining
        .load(std::sync::atomic::Ordering::SeqCst)
}

pub(crate) async fn prepare(
    runtime: &RuntimeAuthority,
    run: &str,
) -> Result<ReplyOutputInspection, RuntimeError> {
    runtime
        .native
        .as_ref()
        .unwrap()
        .prepare_output(runtime, run, runtime.output_fixture.as_ref().unwrap())
        .await
}

pub(crate) async fn receive(
    runtime: &RuntimeAuthority,
    run: &str,
) -> Result<ReplyReceiptProgress, RuntimeError> {
    runtime
        .native
        .as_ref()
        .unwrap()
        .reconcile_output(runtime, run, runtime.output_fixture.as_ref().unwrap())
        .await
}

pub(crate) async fn publication(runtime: &RuntimeAuthority, run: &str) -> roda_proto::Envelope {
    let trx = runtime.execution.transaction().await.unwrap();
    let (record, original) = runtime
        .execution
        .reply_in(&trx, run, &runtime.custody)
        .await
        .unwrap();
    let capture = runtime
        .execution
        .output_capture_in(&trx, &record, &original, &runtime.custody)
        .await
        .unwrap();
    let p = runtime
        .execution
        .publication_in(
            &trx,
            &record,
            &original,
            &capture.association.digest,
            &runtime.custody,
        )
        .await
        .unwrap()
        .unwrap();
    let env = runtime
        .output_fixture
        .as_ref()
        .unwrap()
        .observed
        .lock()
        .unwrap()
        .get(&p.value.publish)
        .unwrap()
        .clone();
    assert_eq!(
        hash(&roda_proto::ClientFrame::Publish { env: env.clone() }.encode()),
        p.value.publish
    );
    env
}

pub(crate) async fn association(runtime: &RuntimeAuthority, attempt: &str) -> Option<String> {
    runtime
        .execution
        .transaction()
        .await
        .unwrap()
        .get(
            &runtime.execution.attempt(attempt, "native-capture/1"),
            false,
        )
        .await
        .unwrap()
        .map(|bytes| hash(&bytes))
}

pub(crate) async fn fingerprints(
    runtime: &RuntimeAuthority,
    run: &str,
) -> (String, String, String, Option<String>) {
    let trx = runtime.execution.transaction().await.unwrap();
    let (record, original) = runtime
        .execution
        .reply_in(&trx, run, &runtime.custody)
        .await
        .unwrap();
    let capture = runtime
        .execution
        .output_capture_in(&trx, &record, &original, &runtime.custody)
        .await
        .unwrap();
    let p = runtime
        .execution
        .publication_in(
            &trx,
            &record,
            &original,
            &capture.association.digest,
            &runtime.custody,
        )
        .await
        .unwrap();
    (
        original,
        capture.association.digest,
        capture.association.value.evidence,
        p.map(|p| p.digest),
    )
}

pub(crate) async fn assert_pinned(runtime: &RuntimeAuthority, run: &str) {
    let trx = runtime.execution.transaction().await.unwrap();
    let (record, original) = runtime
        .execution
        .reply_in(&trx, run, &runtime.custody)
        .await
        .unwrap();
    let capture = runtime
        .execution
        .output_capture_in(&trx, &record, &original, &runtime.custody)
        .await
        .unwrap();
    let p = runtime
        .execution
        .publication_in(
            &trx,
            &record,
            &original,
            &capture.association.digest,
            &runtime.custody,
        )
        .await
        .unwrap()
        .unwrap();
    assert!(trx
        .get(
            &runtime
                .execution
                .native_references(&record.original)
                .pack(&(run,)),
            false
        )
        .await
        .unwrap()
        .is_some());
    assert!(trx
        .get(
            &runtime
                .execution
                .native_references(&p.value.root)
                .pack(&("publication/1", run)),
            false
        )
        .await
        .unwrap()
        .is_some());
    assert!(trx
        .get(&runtime.execution.run(run, "cancel"), false)
        .await
        .unwrap()
        .is_none());
    assert!(trx
        .get(
            &runtime.execution.attempt(&record.locator.attempt, "closed"),
            false
        )
        .await
        .unwrap()
        .is_none());
    drop(trx);
    let principal = &record.original.context.principal;
    let prefix = runtime.execution.root.subspace(&(
        "native-retired",
        principal.agent.as_str(),
        principal.device.as_str(),
    ));
    let trx = runtime.execution.transaction().await.unwrap();
    let rows = super::super::super::storage::bounded_rows(
        &trx,
        foundationdb::RangeOption::from(prefix.range()),
        16,
        8192,
    )
    .await
    .unwrap();
    for (key, bytes) in rows {
        let mut retired: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        retired["collect_after"] = serde_json::json!(trx.get_read_version().await.unwrap());
        trx.set(&key, &serde_json::to_vec(&retired).unwrap());
    }
    trx.commit().await.unwrap();
    runtime
        .execution
        .collect_native_retired(principal)
        .await
        .unwrap();
    let trx = runtime.execution.transaction().await.unwrap();
    for root in [&record.original, &p.value.root] {
        let manifest = runtime.execution.root.pack(&(
            "native-image",
            principal.agent.as_str(),
            principal.device.as_str(),
            root.context.capsule.as_str(),
            "manifest",
        ));
        assert!(
            trx.get(&manifest, false).await.unwrap().is_some(),
            "real GC must keep both protected original and publication images"
        );
    }
}

pub(crate) async fn substitutions(
    runtime: &RuntimeAuthority,
    step: &VerifiedStep,
    fixture: &VerifiedStep,
) {
    let VerifiedStep::Native(step) = step else {
        panic!("real native paid step required");
    };
    let trx = runtime.execution.transaction().await.unwrap();
    let retained = runtime
        .execution
        .output_capture_in(
            &trx,
            &step.loaded.record,
            &step.loaded.record_digest,
            &runtime.custody,
        )
        .await
        .unwrap();
    drop(trx);
    let nonce = &retained.association.value.admitted_nonce;
    let capability = runtime.output_fixture.as_ref().unwrap();
    for mutation in 0..4 {
        let mut capture = Capture {
            binding: retained.capture.binding.clone(),
            result: retained.capture.result.clone(),
        };
        match mutation {
            0 => capture.binding.frontier = hash(b"different binding"),
            1 => capture.result.request_digest = hash(b"different original request"),
            2 => capture.result.context.attempt_id = roda_types::new_id("different-attempt"),
            _ => {
                capture.result.output = Ok(vec![OutputBlock::Text {
                    text: "Different output with otherwise exact original context".into(),
                }])
            }
        }
        let (digest, sealed) = runtime
            .custody
            .seal(
                &format!("evidence/{}", retained.capture.binding.context.attempt_id),
                &capture,
            )
            .unwrap();
        assert_eq!(
            runtime
                .execution
                .associate_native_capture(
                    capability,
                    &step.loaded,
                    &capture,
                    nonce,
                    &digest,
                    &sealed,
                    &runtime.custody
                )
                .await
                .unwrap_err(),
            RuntimeError::InvalidBinding
        );
    }
    let (evidence, sealed) = runtime
        .custody
        .seal(
            &format!("evidence/{}", retained.capture.binding.context.attempt_id),
            &retained.capture,
        )
        .unwrap();
    assert_eq!(
        associate_capture(
            runtime,
            fixture,
            &retained.capture,
            nonce,
            &evidence,
            &sealed
        )
        .await
        .unwrap_err(),
        RuntimeError::Denied
    );
    runtime
        .execution
        .associate_native_capture(
            capability,
            &step.loaded,
            &retained.capture,
            nonce,
            &evidence,
            &sealed,
            &runtime.custody,
        )
        .await
        .unwrap();
    assert_eq!(
        association(runtime, &step.loaded.record.locator.attempt)
            .await
            .as_deref(),
        Some(retained.association.digest.as_str())
    );
}

pub(crate) async fn metadata_bounds(runtime: &RuntimeAuthority, run: &str) {
    let trx = runtime.execution.transaction().await.unwrap();
    let (record, original) = runtime
        .execution
        .reply_in(&trx, run, &runtime.custody)
        .await
        .unwrap();
    let retained = runtime
        .execution
        .output_capture_in(&trx, &record, &original, &runtime.custody)
        .await
        .unwrap();
    let mut p = runtime
        .execution
        .publication_in(
            &trx,
            &record,
            &original,
            &retained.association.digest,
            &runtime.custody,
        )
        .await
        .unwrap()
        .unwrap()
        .value;
    let bound = "a".repeat(128);
    let mut intent = serde_json::to_value(&p.intent).unwrap();
    for key in ["agent", "owner", "space", "grant"] {
        intent[key] = "a".repeat(256).into();
    }
    p.intent = serde_json::from_value(intent).unwrap();
    p.run = bound.clone();
    p.client_id = bound.clone();
    p.message = bound.clone();
    p.root.context.principal.owner = bound.clone();
    p.root.context.principal.agent = bound.clone();
    p.root.context.principal.device = bound.clone();
    p.root.context.capsule = bound.clone();
    p.root.context.previous = Some(hash(b"bounded previous"));
    p.root.context.generation = u64::MAX;
    p.root.context.image_bytes = IMAGE_BYTES as u32;
    p.root.context.chunks = MAX_CHUNKS as u32;
    let c = NativeCaptureRecord {
        format: 1,
        source: bound.clone(),
        run: bound.clone(),
        attempt: bound.clone(),
        original: original.clone(),
        binding: hash(b"bounded binding"),
        original_capsule: hash(b"bounded capsule"),
        admitted_nonce: bound,
        evidence: hash(b"bounded capture"),
    };
    let r = ReceiptRecord {
        format: 1,
        original,
        capture: hash(b"bounded capture association"),
        publication: hash(b"bounded publication association"),
        reference: p.reference.clone(),
        publish: p.publish.clone(),
        accepted: Seen {
            seq: u64::MAX,
            hash: hash(b"bounded accepted"),
        },
        source: Seen {
            seq: u64::MAX,
            hash: hash(b"bounded head"),
        },
        root: p.root.clone(),
    };
    let sizes = [
        serde_json::to_vec(&c).unwrap().len(),
        serde_json::to_vec(&p).unwrap().len(),
        serde_json::to_vec(&r).unwrap().len(),
    ];
    assert!(sizes.iter().all(|s| *s < METADATA_BYTES));
    assert!(
        seal_metadata(&runtime.custody, "bounded-capture", c)
            .unwrap()
            .bytes
            .len()
            < crate::custody::MAX_SEALED
    );
    assert!(
        seal_metadata(&runtime.custody, "bounded-publication", p)
            .unwrap()
            .bytes
            .len()
            < crate::custody::MAX_SEALED
    );
    assert!(
        seal_metadata(&runtime.custody, "bounded-receipt", r)
            .unwrap()
            .bytes
            .len()
            < crate::custody::MAX_SEALED
    );
    assert!(matches!(
        seal_metadata(
            &runtime.custody,
            "oversized-metadata",
            "a".repeat(METADATA_BYTES)
        ),
        Err(RuntimeError::EvidenceTooLarge)
    ));
    println!("native output metadata fixture: bounded-field capture/publication/receipt encoded bytes {sizes:?}, 16384 plaintext and98304 custody caps unchanged; no text/body/raw Publish in metadata");
}

pub(crate) async fn takeover(runtime: &RuntimeAuthority, run: &str) {
    let trx = runtime.execution.transaction().await.unwrap();
    let (record, original) = runtime
        .execution
        .reply_in(&trx, run, &runtime.custody)
        .await
        .unwrap();
    let version = trx.get_read_version().await.unwrap();
    let mut prior_tokens = Vec::new();
    for key in [
        runtime.execution.run(run, "lease"),
        runtime
            .execution
            .native_lease(&record.original.context.principal),
    ] {
        let bytes = trx.get(&key, false).await.unwrap().unwrap();
        let mut lease: crate::execution::Lease = serde_json::from_slice(&bytes).unwrap();
        prior_tokens.push(lease.token);
        lease.expires = version;
        trx.set(&key, &serde_json::to_vec(&lease).unwrap());
    }
    trx.commit().await.unwrap();
    let trx = runtime.execution.transaction().await.unwrap();
    runtime
        .execution
        .reply_original_scope(&trx, &record, &original)
        .await
        .unwrap();
    let successor = runtime
        .execution
        .acquire_reply_in(&trx, &record)
        .await
        .unwrap()
        .unwrap();
    trx.commit().await.unwrap();
    assert_eq!(successor.run.token, prior_tokens[0] + 1);
    assert_eq!(successor.device.token, prior_tokens[1] + 1);
    runtime
        .execution
        .release_reply(&record, &successor)
        .await
        .unwrap();
}
