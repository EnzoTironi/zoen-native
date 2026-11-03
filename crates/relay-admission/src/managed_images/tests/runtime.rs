use super::*;
use event_log::{Author, Signer};
use protocol::{Envelope, Sealed, SealedKind};

fn configured() -> RuntimeDeployment {
    RuntimeDeployment {
        namespace: "shared-runtime".into(),
        witness: "a".repeat(64),
        source: "relay-cell".into(),
    }
}

fn wire(agent: u8, device: u8, client: &str) -> Result<Envelope, Box<dyn std::error::Error>> {
    let author = Author::device(
        &Signer::from_secret(&[agent; 32]),
        Signer::from_secret(&[device; 32]),
    );
    Ok(Envelope::sealed(
        &author,
        "shared-space",
        client,
        1,
        None,
        Sealed::new(SealedKind::Application, 1, vec![1, 2, 3]),
    )?)
}

fn committed(writer: &mut AnchorWriter, envelope: &Envelope) -> Result<Checkpoint, AnchorError> {
    super::publish::commit(writer, &publish_digest(&configured().namespace, envelope)?)
}

#[test]
fn legacy_checkpoint_serialization_and_commitment_remain_byte_identical() -> TestResult {
    let home = Home::new()?;
    let writer = AnchorWriter::create(&home.0, deployment())?;
    let frozen = br#"{"schema":2,"deployment":{"namespace":"fixture","witness":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","source":"relay-cell","agent":"agent","device":"device"},"committed":{"generation":1,"sequence":0,"digest":"8de7453d2ff0f4b4489135d01f9fdef8fc56f930e3a0b4df5a55f1968db43611"},"committed_effect":null,"prepared":null}"#;
    assert_eq!(std::fs::read(home.0.join("checkpoint.json"))?, frozen);
    drop(writer);
    AnchorWriter::open(&home.0, deployment())?;
    Ok(())
}

#[test]
fn distinct_agents_and_devices_share_history_without_sharing_a_publication_permit() -> TestResult {
    let home = Home::new()?;
    let mut writer = AnchorWriter::create_runtime(&home.0, configured())?;
    let genesis = writer.reader().checkpoint()?;
    // Same client ID and payload, different agent/device. Identity is part of
    // the original signed wire, not a separate per-agent counter or balance.
    let first = wire(41, 42, "same-client")?;
    let sibling = wire(41, 43, "same-client")?;
    let other = wire(44, 45, "same-client")?;
    let mut originals = Vec::new();
    for envelope in [&first, &sibling, &other] {
        let checkpoint = committed(&mut writer, envelope)?;
        let ConsumePublish::Fresh(permit) =
            writer.reader().consume_publish(&checkpoint, envelope)?
        else {
            return Err("a distinct original wire must have one fresh permit".into());
        };
        assert_eq!(permit.condition().agent, envelope.author());
        assert_eq!(Some(permit.condition().device.as_str()), envelope.device());
        permit.check(envelope)?;
        for substituted in [&first, &sibling, &other] {
            if substituted.wire_hash() != envelope.wire_hash() {
                assert!(permit.check(substituted).is_err());
                assert!(writer
                    .reader()
                    .consume_publish(&checkpoint, substituted)
                    .is_err());
            }
        }
        originals.push((checkpoint, permit));
    }
    let tip = writer.reader().checkpoint()?;
    assert_eq!(tip.sequence, 3);
    assert_eq!(tip.generation, genesis.generation);
    for ((checkpoint, permit), envelope) in originals.iter().zip([&first, &sibling, &other]) {
        permit.check(envelope)?;
        permit.check_store_checkpoint(&tip)?;
        assert!(permit.check_store_checkpoint(&genesis).is_err());
        assert_eq!(
            writer.reader().retained_checkpoint(checkpoint)?.next,
            *checkpoint
        );
    }
    // Retirement drains every principal's actual in-flight effect.
    assert_eq!(
        writer.prepare(&effect(), &tip, &tip, true),
        Err(AnchorError::Busy)
    );
    drop(originals);
    drop(writer);
    let mut cold = AnchorWriter::open_runtime(&home.0, configured())?;
    assert_eq!(
        cold.reconcile(&genesis, &genesis),
        Err(AnchorError::Unreconciled)
    );
    for envelope in [&first, &sibling, &other] {
        let checkpoint = committed(&mut cold, envelope)?;
        let ConsumePublish::Consumed(observed) =
            cold.reader().consume_publish(&checkpoint, envelope)?
        else {
            return Err("restart or later progress recreated an already consumed permit".into());
        };
        assert_eq!(observed.agent, envelope.author());
        assert_eq!(Some(observed.device.as_str()), envelope.device());
        assert!(observed.checkpoint.sequence <= 3);
    }
    let before = cold.reader().checkpoint()?;
    let retirement = cold.prepare(&effect(), &before, &before, true)?;
    cold.reconcile(&retirement.next, &retirement.next)?;
    assert_eq!(retirement.next.generation, before.generation + 1);
    assert!(cold.reader().historical_generation_guard(&tip).is_err());
    Ok(())
}

#[test]
fn device_and_runtime_formats_cannot_be_adopted_or_rebound() -> TestResult {
    let home = Home::new()?;
    let writer = AnchorWriter::create_runtime(&home.0, configured())?;
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(home.0.join("checkpoint.json"))?)?;
    assert_eq!(state["schema"], 3);
    assert!(state["deployment"].get("agent").is_none());
    assert!(!format!("{:?}", configured()).contains(&configured().witness));
    let old = Deployment {
        namespace: configured().namespace,
        witness: configured().witness,
        source: configured().source,
        agent: "agent".into(),
        device: "device".into(),
    };
    assert!(AnchorReader::open(&home.0, old.clone()).is_err());
    drop(writer);
    for changed in [
        RuntimeDeployment {
            namespace: "other".into(),
            ..configured()
        },
        RuntimeDeployment {
            witness: "b".repeat(64),
            ..configured()
        },
        RuntimeDeployment {
            source: "other-cell".into(),
            ..configured()
        },
    ] {
        assert!(AnchorWriter::open_runtime(&home.0, changed).is_err());
    }
    let legacy = Home::new()?;
    let writer = AnchorWriter::create(&legacy.0, old)?;
    assert!(AnchorReader::open_runtime(&legacy.0, configured()).is_err());
    let mut state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(legacy.0.join("checkpoint.json"))?)?;
    state["schema"] = 3.into();
    std::fs::write(
        legacy.0.join("checkpoint.json"),
        serde_json::to_vec(&state)?,
    )?;
    assert!(writer.reader().checkpoint().is_err());
    Ok(())
}

#[test]
fn every_partial_runtime_commit_retains_the_original_transition_after_cold_open() -> TestResult {
    for sql_first in [false, true] {
        let home = Home::new()?;
        let mut writer = AnchorWriter::create_runtime(&home.0, configured())?;
        let before = writer.reader().checkpoint()?;
        let pending = writer.prepare(&effect(), &before, &before, false)?;
        drop(writer);
        let mut cold = AnchorWriter::open_runtime(&home.0, configured())?;
        let (sql, fdb, recovery) = if sql_first {
            (&pending.next, &before, Recovery::FdbBefore)
        } else {
            (&before, &pending.next, Recovery::SqlBefore)
        };
        assert_eq!(cold.reconcile(sql, fdb)?, recovery);
        assert_eq!(cold.pending()?, Some(pending.clone()));
        assert!(cold.reader().checkpoint().is_err());
        assert!(cold.prepare(&effect(), sql, fdb, false).is_err());
        assert_eq!(
            cold.reconcile(&pending.next, &pending.next)?,
            Recovery::BothAfter
        );
    }
    Ok(())
}
