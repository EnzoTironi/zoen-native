use super::*;
use relay_admission::managed_images::{publish_digest, ConsumePublish, RuntimeDeployment};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn setup() -> Result<(Home, ManagedRuntime, AnchorWriter), Box<dyn std::error::Error>> {
    let home = Home(std::env::temp_dir().join(types::new_id("shared-relay")?));
    let runtime = ManagedRuntime {
        deployment: RuntimeDeployment {
            namespace: "shared-runtime".into(),
            witness: "a".repeat(64),
            source: "test-cell".into(),
        },
        anchor_directory: home.0.clone(),
        principals: vec![
            ManagedPrincipal {
                agent: "first-agent".into(),
                device: "b".repeat(64),
            },
            ManagedPrincipal {
                agent: "second-agent".into(),
                device: "c".repeat(64),
            },
        ],
    };
    let writer = AnchorWriter::create_runtime(&home.0, runtime.deployment.clone())?;
    Ok((home, runtime, writer))
}

#[test]
fn explicitly_registered_agents_observe_one_checkpoint_and_one_retirement() -> TestResult {
    use event_log::{Author, Signer};
    let (_home, mut runtime, mut writer) = setup()?;
    let authors = [
        Author::device(&Signer::generate()?, Signer::generate()?),
        Author::device(&Signer::generate()?, Signer::generate()?),
    ];
    runtime.principals = authors
        .iter()
        .map(|author| {
            Ok(ManagedPrincipal {
                agent: author.identity.clone(),
                device: author.device.clone().ok_or("missing certified device")?,
            })
        })
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;
    let registry = ManagedDevices::open_with_runtimes([], [runtime.clone()])?;
    let first = registry
        .get(&runtime.principals[0].device)
        .ok_or("first registration")?;
    let second = registry
        .get(&runtime.principals[1].device)
        .ok_or("second registration")?;
    assert_eq!(first.deployment().agent, authors[0].identity);
    assert_eq!(second.deployment().agent, authors[1].identity);
    assert!(registry.get(&"d".repeat(64)).is_none());
    assert!(registry.source_matches("test-cell"));
    assert!(!registry.source_matches("other-cell"));
    let mut guards = Vec::new();
    for (author, configured) in authors.iter().zip([&first, &second]) {
        let envelope = protocol::Envelope::sealed(
            author,
            "test-space",
            "same-id",
            1,
            None,
            protocol::Sealed::new(protocol::SealedKind::Application, 1, vec![5; 32]),
        )?;
        let before = writer.reader().checkpoint()?;
        let pending = writer.prepare(
            &publish_digest(&runtime.deployment.namespace, &envelope)?,
            &before,
            &before,
            false,
        )?;
        writer.reconcile(&pending.next, &pending.next)?;
        let ConsumePublish::Fresh(permit) = configured
            .anchor()
            .consume_publish(&pending.next, &envelope)?
        else {
            return Err("registered principal must consume exactly one original permit".into());
        };
        assert_eq!(permit.condition().agent, author.identity);
        permit.check(&envelope)?;
        guards.push(permit);
    }
    let tip = writer.reader().checkpoint()?;
    assert_eq!(first.anchor().checkpoint()?, tip);
    assert_eq!(second.anchor().checkpoint()?, tip);
    assert_eq!(
        writer.prepare(&"d".repeat(64), &tip, &tip, true),
        Err(AnchorError::Busy)
    );
    drop(guards);
    let retirement = writer.prepare(&"d".repeat(64), &tip, &tip, true)?;
    writer.reconcile(&retirement.next, &retirement.next)?;
    assert!(first.upload_guard(&tip).is_err());
    assert!(second.upload_guard(&tip).is_err());
    assert_eq!(first.anchor().checkpoint()?, retirement.next);
    Ok(())
}

#[test]
fn runtime_registry_rejects_duplicate_namespace_device_wrong_pairing_or_empty_principals(
) -> TestResult {
    let (_home, runtime, _writer) = setup()?;
    let mut duplicate_namespace = runtime.clone();
    duplicate_namespace.principals = vec![ManagedPrincipal {
        agent: "third-agent".into(),
        device: "d".repeat(64),
    }];
    assert!(ManagedDevices::open_with_runtimes([], [duplicate_namespace.clone()]).is_ok());
    assert!(
        ManagedDevices::open_with_runtimes([], [runtime.clone(), duplicate_namespace]).is_err()
    );
    let mut duplicated = runtime.clone();
    duplicated.principals.push(duplicated.principals[0].clone());
    assert!(ManagedDevices::open_with_runtimes([], [duplicated]).is_err());
    let mut empty = runtime.clone();
    empty.principals.clear();
    assert!(ManagedDevices::open_with_runtimes([], [empty]).is_err());
    let mut wrong = runtime.clone();
    wrong.deployment.witness = "e".repeat(64);
    assert!(ManagedDevices::open_with_runtimes([], [wrong]).is_err());
    let mut oversized = runtime.clone();
    oversized.principals = vec![runtime.principals[0].clone(); 257];
    assert!(ManagedDevices::open_with_runtimes([], [oversized]).is_err());
    let (legacy_home, legacy, _legacy_writer) = fixture()?;
    let mut overlap = runtime;
    overlap.deployment.namespace.clone_from(&legacy.namespace);
    let overlap_home = Home(std::env::temp_dir().join(types::new_id("mixed-profile")?));
    overlap.anchor_directory.clone_from(&overlap_home.0);
    let _overlap_writer =
        AnchorWriter::create_runtime(&overlap_home.0, overlap.deployment.clone())?;
    assert!(ManagedDevices::open_with_runtimes([], [overlap.clone()]).is_ok());
    assert!(
        ManagedDevices::open_with_runtimes([(legacy, legacy_home.0.clone())], [overlap]).is_err()
    );
    Ok(())
}
