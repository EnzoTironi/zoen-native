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
    let custody = runtime.native.as_ref().unwrap();
    let credential = custody.credential(agent, device).unwrap();
    let mut core =
        DeviceCore::bootstrap(credential.unlocked().unwrap(), "http://relay.fixture").unwrap();
    let (packages, image) = core.key_packages(8).unwrap();
    assert_eq!(packages.len(), 8);
    let principal = credential.principal();
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
    runtime
        .execution
        .activate_native(staged, &fence)
        .await
        .unwrap();
    guard.commit().await.unwrap();
    runtime
        .execution
        .release_native_device(&principal, &fence)
        .await
        .unwrap();
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
