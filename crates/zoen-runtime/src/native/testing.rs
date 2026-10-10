//! Explicit fixture custody, not a production key or enrollment constructor.
use super::*;

pub(crate) fn custody(
    namespace: &str,
    agent: Identity,
    owner: Identity,
    certificate: String,
    secret: [u8; 32],
) -> NativeCustody {
    let credential = Credential {
        agent,
        owner,
        certificate,
        secret: Zeroizing::new(secret),
    };
    credential.unlocked().unwrap();
    let principal = credential.principal();
    NativeCustody {
        cipher: Custody::new([73; 32], format!("native-capsule/1/{namespace}")),
        key_version: 1,
        credentials: BTreeMap::from([((principal.agent, principal.device), credential)]),
    }
}

pub(crate) async fn provision(runtime: &RuntimeAuthority, agent: &str, device: &str) {
    assert_eq!(provision_packages(runtime, agent, device).await.len(), 8);
}

pub(crate) fn add_device(
    custody: &mut NativeCustody,
    agent: Identity,
    owner: Identity,
    certificate: String,
    secret: [u8; 32],
) {
    let credential = Credential {
        agent,
        owner,
        certificate,
        secret: Zeroizing::new(secret),
    };
    credential.unlocked().unwrap();
    let principal = credential.principal();
    assert!(custody
        .credentials
        .insert((principal.agent, principal.device), credential)
        .is_none());
}

pub(crate) async fn provision_packages(
    runtime: &RuntimeAuthority,
    agent: &str,
    device: &str,
) -> Vec<Vec<u8>> {
    let custody = runtime.native.as_ref().unwrap();
    let credential = custody.credential(agent, device).unwrap();
    let mut core =
        DeviceCore::bootstrap(credential.unlocked().unwrap(), "http://relay.fixture").unwrap();
    let (packages, image) = core.key_packages(8).unwrap();
    assert_eq!(packages.len(), 8);
    let principal = credential.principal();
    let fence = runtime
        .execution
        .acquire_native_device(&principal)
        .await
        .unwrap();
    let candidate = custody.seal_image(&principal, None, &image).unwrap();
    assert!(
        candidate.chunks.len() > 1,
        "real complete provider image spans multiple chunks"
    );
    let staged = runtime
        .execution
        .stage_native(&principal, None, &fence, candidate)
        .await
        .unwrap();
    let admission = runtime
        .finance
        .native_directory(
            &credential.agent,
            &credential.owner,
            device,
            &credential.certificate,
        )
        .await
        .unwrap()
        .authorize(staged.scope(&runtime.execution, &fence))
        .await
        .unwrap();
    runtime
        .execution
        .activate_native(staged, &fence, admission)
        .await
        .unwrap();
    runtime
        .execution
        .release_native_device(&principal, &fence)
        .await
        .unwrap();
    packages
}

pub(crate) async fn expire_abandoned_stage(runtime: &RuntimeAuthority, agent: &str, device: &str) {
    // Explicit version-clock fixture cut, not elapsed retention or a wire fault.
    let trx = runtime.execution.transaction().await.unwrap();
    let key = runtime
        .execution
        .root
        .pack(&("native-device", agent, device, "stage"));
    let value = trx.get(&key, false).await.unwrap().unwrap();
    let mut stage: serde_json::Value = serde_json::from_slice(&value).unwrap();
    stage["expires"] = serde_json::json!(trx.get_read_version().await.unwrap() - 61_000_000);
    trx.set(&key, &serde_json::to_vec(&stage).unwrap());
    trx.commit().await.unwrap();
    let principal = runtime
        .native
        .as_ref()
        .unwrap()
        .credential(agent, device)
        .unwrap()
        .principal();
    runtime
        .execution
        .collect_native_stage(&principal)
        .await
        .unwrap();
}

pub(crate) async fn reply_original(runtime: &RuntimeAuthority, run: &str) -> (String, String) {
    super::runs::testing::original(runtime, run).await
}

pub(crate) async fn reply_fences(runtime: &RuntimeAuthority, run: &str) {
    super::runs::testing::fences(runtime, run).await;
}

pub(crate) async fn storage_cuts(runtime: &RuntimeAuthority, agent: &str, device: &str) {
    let custody = runtime.native.as_ref().unwrap();
    let credential = custody.credential(agent, device).unwrap();
    let principal = credential.principal();
    let initial = runtime
        .execution
        .read_native(&principal, None)
        .await
        .unwrap();
    let root = initial.root.clone();
    let image = custody.open_image(&principal, initial).unwrap();
    let expected = image.as_bytes().to_vec();
    for mutation in 0..13 {
        let mut candidate = custody.seal_image(&principal, Some(&root), &image).unwrap();
        match mutation {
            0 => candidate.chunks.swap(0, 1),
            1 => candidate.chunks[0][40] ^= 1,
            2 => {
                candidate.chunks.remove(0);
            }
            3 => candidate.chunks[0].push(0),
            4 => candidate.manifest[30] ^= 1,
            5 => candidate.root.context.key_version += 1,
            6 => candidate.root.context.previous = Some(hash(b"other parent")),
            7 => candidate.root.context.generation += 1,
            8 => candidate.root.context.principal.certificate_digest = hash(b"other certificate"),
            9 => candidate.root.context.image_bytes = u32::MAX,
            10 => candidate.root.context.chunks = u32::MAX,
            11 => candidate.manifest.resize(MANIFEST_BYTES, 0),
            12 => candidate.root.context.principal.device = roda_log::Signer::generate().id(),
            _ => unreachable!(),
        }
        assert!(
            custody.open_image(&principal, candidate).is_err(),
            "mutation {mutation}"
        );
    }
    let candidate = custody.seal_image(&principal, Some(&root), &image).unwrap();
    assert_eq!(
        custody
            .open_image(&principal, candidate)
            .unwrap()
            .as_bytes(),
        expected
    );
    for (key, namespace) in [([74; 32], "wrong-key"), ([73; 32], "wrong-namespace")] {
        let other = NativeCustody {
            cipher: Custody::new(key, namespace.into()),
            key_version: 1,
            credentials: BTreeMap::new(),
        };
        assert!(other
            .open_image(
                &principal,
                custody.seal_image(&principal, Some(&root), &image).unwrap()
            )
            .is_err());
    }
    assert!(NativeImage::decode_bounded(vec![0; IMAGE_BYTES + 1]).is_err());
    storage::test_cuts(&runtime.execution, custody, &principal, &image).await;
    println!("native custody journey: binary chunks, authenticated complete restore and fenced activation cuts PASS");
}

pub(crate) async fn generation(runtime: &RuntimeAuthority, agent: &str, device: &str) -> u64 {
    let principal = runtime
        .native
        .as_ref()
        .unwrap()
        .credential(agent, device)
        .unwrap()
        .principal();
    runtime
        .execution
        .read_native(&principal, None)
        .await
        .unwrap()
        .root
        .context
        .generation
}

pub(crate) async fn authorization_cuts(runtime: &RuntimeAuthority, agent: &str, device: &str) {
    let custody = runtime.native.as_ref().unwrap();
    let credential = custody.credential(agent, device).unwrap();
    let principal = credential.principal();
    let fence = runtime
        .execution
        .acquire_native_device(&principal)
        .await
        .unwrap();
    let capsule = runtime
        .execution
        .read_native(&principal, Some(&fence))
        .await
        .unwrap();
    let previous = capsule.root.clone();
    let image = custody.open_image(&principal, capsule).unwrap();
    let candidate = custody
        .seal_image(&principal, Some(&previous), &image)
        .unwrap();
    let staged = runtime
        .execution
        .stage_native(&principal, Some(previous.clone()), &fence, candidate)
        .await
        .unwrap();
    let exact = staged.scope(&runtime.execution, &fence);
    for mutation in 0..15 {
        let mut wrong = exact.clone();
        match mutation {
            0 => wrong.namespace.push(0),
            1 => wrong.deployment = hash(b"other deployment"),
            2 => wrong.target.context.principal.owner = roda_log::Signer::generate().id(),
            3 => wrong.target.context.principal.agent = roda_log::Signer::generate().id(),
            4 => wrong.target.context.principal.device = roda_log::Signer::generate().id(),
            5 => wrong.target.context.principal.certificate_digest = hash(b"other certificate"),
            6 => wrong.target.context.capsule = roda_types::new_id("capsule"),
            7 => wrong.target.context.generation += 1,
            8 => wrong.target.manifest_digest = hash(b"other manifest"),
            9 => wrong.target.context.image_digest = hash(b"other image"),
            10 => {
                wrong.previous.as_mut().unwrap().manifest_digest = hash(b"other previous manifest")
            }
            11 => wrong.previous.as_mut().unwrap().context.capsule = roda_types::new_id("capsule"),
            12 => wrong.fence.holder = roda_types::new_id("other holder"),
            13 => wrong.fence.token += 1,
            14 => wrong.previous = None,
            _ => unreachable!(),
        }
        let guard = runtime
            .finance
            .native_directory(
                &credential.agent,
                &credential.owner,
                device,
                &credential.certificate,
            )
            .await
            .unwrap();
        let error = match guard.authorize(wrong).await {
            Err(error) => error,
            Ok(admission) => runtime
                .execution
                .activate_native(staged.clone(), &fence, admission)
                .await
                .unwrap_err(),
        };
        assert_eq!(error, RuntimeError::InvalidBinding, "mutation {mutation}");
        assert_eq!(
            generation(runtime, agent, device).await,
            previous.context.generation
        );
    }
    let guard = runtime
        .finance
        .native_directory(
            &credential.agent,
            &credential.owner,
            device,
            &credential.certificate,
        )
        .await
        .unwrap();
    assert!(matches!(
        guard.suppress_known_ack().authorize(exact.clone()).await,
        Err(RuntimeError::Unavailable)
    ));
    assert_eq!(
        generation(runtime, agent, device).await,
        previous.context.generation
    );
    let guard = runtime
        .finance
        .native_directory(
            &credential.agent,
            &credential.owner,
            device,
            &credential.certificate,
        )
        .await
        .unwrap();
    assert!(matches!(
        guard.delay_known_ack().authorize(exact.clone()).await,
        Err(RuntimeError::Denied)
    ));
    assert_eq!(
        generation(runtime, agent, device).await,
        previous.context.generation
    );
    let guard = runtime
        .finance
        .native_directory(
            &credential.agent,
            &credential.owner,
            device,
            &credential.certificate,
        )
        .await
        .unwrap();
    let admission = guard.authorize(exact.clone()).await.unwrap();
    // An actual elapsed post-ACK pause exceeds the original two-second window.
    tokio::time::sleep(std::time::Duration::from_millis(2200)).await;
    assert_eq!(
        runtime
            .execution
            .activate_native(staged.clone(), &fence, admission)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(
        generation(runtime, agent, device).await,
        previous.context.generation
    );
    // A fresh live SQL authorization is required; reading the root minted none.
    let guard = runtime
        .finance
        .native_directory(
            &credential.agent,
            &credential.owner,
            device,
            &credential.certificate,
        )
        .await
        .unwrap();
    let admission = guard.authorize(exact).await.unwrap();
    let root = runtime
        .execution
        .activate_native(staged, &fence, admission)
        .await
        .unwrap();
    assert_eq!(root.context.generation, previous.context.generation + 1);
    runtime
        .execution
        .release_native_device(&principal, &fence)
        .await
        .unwrap();
    println!("native maintenance admission cuts: 15 exact-scope mutations refused; known SQL ACK suppression/delay and actual post-ACK expiry preserve root PASS");
}

pub(crate) async fn admitted_before_revocation(
    runtime: &RuntimeAuthority,
    agent: &str,
    device: &str,
) {
    let custody = runtime.native.as_ref().unwrap();
    let credential = custody.credential(agent, device).unwrap();
    let principal = credential.principal();
    let fence = runtime
        .execution
        .acquire_native_device(&principal)
        .await
        .unwrap();
    let capsule = runtime
        .execution
        .read_native(&principal, Some(&fence))
        .await
        .unwrap();
    let previous = capsule.root.clone();
    let image = custody.open_image(&principal, capsule).unwrap();
    let staged = runtime
        .execution
        .stage_native(
            &principal,
            Some(previous.clone()),
            &fence,
            custody
                .seal_image(&principal, Some(&previous), &image)
                .unwrap(),
        )
        .await
        .unwrap();
    let guard = runtime
        .finance
        .native_directory(
            &credential.agent,
            &credential.owner,
            device,
            &credential.certificate,
        )
        .await
        .unwrap();
    let admission = guard
        .authorize(staged.scope(&runtime.execution, &fence))
        .await
        .unwrap();
    // Maintenance was admitted at the known SQL commit. This later revocation
    // cannot retroactively cancel that finite admitted operation.
    sqlx::query("UPDATE devices SET revoked_at=clock_timestamp() WHERE device=$1")
        .bind(device)
        .execute(&runtime.finance.pool)
        .await
        .unwrap();
    let root = runtime
        .execution
        .activate_native(staged, &fence, admission)
        .await
        .unwrap();
    assert_eq!(root.context.generation, previous.context.generation + 1);
    runtime
        .execution
        .release_native_device(&principal, &fence)
        .await
        .unwrap();
    assert_eq!(
        runtime
            .inspect_native_device(agent, device)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    assert_eq!(
        runtime
            .repack_native_device(agent, device)
            .await
            .unwrap_err(),
        RuntimeError::Denied
    );
    println!("native maintenance revocation ordering: known pre-revocation SQL admission finishes within window; subsequent calls denied PASS");
}
