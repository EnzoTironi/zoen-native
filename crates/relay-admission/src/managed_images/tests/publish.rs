use super::*;
use event_log::{Author, Signer};
use protocol::{Envelope, Sealed, SealedKind};

fn envelope(client: &str) -> Result<(Deployment, Envelope), Box<dyn std::error::Error>> {
    let identity = Signer::from_secret(&[91; 32]);
    let actor = Author::device(&identity, Signer::from_secret(&[92; 32]));
    let mut configured = deployment();
    configured.agent = actor.identity.clone();
    configured.device = actor.device.clone().unwrap();
    let envelope = Envelope::sealed(
        &actor,
        "space",
        client,
        1,
        None,
        Sealed::new(SealedKind::Application, 1, vec![1, 2, 3]),
    )?;
    Ok((configured, envelope))
}

pub(super) fn commit(writer: &mut AnchorWriter, digest: &str) -> Result<Checkpoint, AnchorError> {
    let previous = writer.reader().checkpoint()?;
    let prepared = writer.prepare(digest, &previous, &previous, false)?;
    assert_eq!(
        writer.reconcile(&prepared.next, &prepared.next)?,
        Recovery::BothAfter
    );
    Ok(prepared.next)
}

#[test]
fn original_publish_is_consumed_once_even_after_restart_and_new_checkpoint() -> TestResult {
    let home = Home::new()?;
    let (configured, envelope) = envelope("original")?;
    let mut writer = AnchorWriter::create(&home.0, configured.clone())?;
    let digest = publish_digest(&configured.namespace, &envelope)?;
    let checkpoint = commit(&mut writer, &digest)?;
    assert_eq!(
        writer.reader().retained_checkpoint(&checkpoint)?.next,
        checkpoint
    );
    let ConsumePublish::Fresh(permit) = writer.reader().consume_publish(&checkpoint, &envelope)?
    else {
        panic!("first original consumption must produce the single permit")
    };
    let original = permit.condition().clone();
    assert_eq!(
        writer.reader().inspect_publish(&checkpoint, &envelope)?,
        original
    );
    permit.check(&envelope)?;
    drop(permit);
    drop(writer);
    let mut restarted = AnchorWriter::open(&home.0, configured)?;
    let newer = commit(&mut restarted, &digest)?;
    assert_ne!(newer, checkpoint);
    assert_eq!(
        restarted.reader().retained_checkpoint(&checkpoint)?.next,
        checkpoint
    );
    let ConsumePublish::Consumed(observation) =
        restarted.reader().consume_publish(&newer, &envelope)?
    else {
        panic!("a new checkpoint must never recreate a consumed original permit")
    };
    assert_eq!(observation, original);
    Ok(())
}

#[test]
fn checkpoint_authorizes_exact_original_wire_only() -> TestResult {
    let home = Home::new()?;
    let (configured, original) = envelope("original")?;
    let (_, other) = envelope("other")?;
    let mut writer = AnchorWriter::create(&home.0, configured.clone())?;
    let genesis = writer.reader().checkpoint()?;
    assert_eq!(
        writer.reader().retained_checkpoint(&genesis),
        Err(AnchorError::Unreconciled)
    );
    assert!(writer
        .reader()
        .consume_publish(&genesis, &original)
        .is_err());
    let dispatch = commit(&mut writer, &effect())?;
    assert!(writer
        .reader()
        .consume_publish(&dispatch, &original)
        .is_err());
    let checkpoint = commit(
        &mut writer,
        &publish_digest(&configured.namespace, &original)?,
    )?;
    assert!(matches!(
        writer.reader().consume_publish(&checkpoint, &other),
        Err(AnchorError::Invalid)
    ));
    assert!(matches!(
        writer.reader().consume_publish(&checkpoint, &original)?,
        ConsumePublish::Fresh(_)
    ));
    Ok(())
}

#[test]
fn unrelated_progress_preserves_pending_permit_but_retirement_drains_it() -> TestResult {
    let home = Home::new()?;
    let (configured, envelope) = envelope("original")?;
    let mut writer = AnchorWriter::create(&home.0, configured.clone())?;
    let checkpoint = commit(
        &mut writer,
        &publish_digest(&configured.namespace, &envelope)?,
    )?;
    let ConsumePublish::Fresh(permit) = writer.reader().consume_publish(&checkpoint, &envelope)?
    else {
        panic!("fresh permit")
    };
    let later = commit(&mut writer, &effect())?;
    permit.check(&envelope)?;
    assert_eq!(
        permit.check_store_checkpoint(&checkpoint),
        Err(AnchorError::Unreconciled)
    );
    permit.check_store_checkpoint(&later)?;
    assert_eq!(
        writer.prepare(&effect(), &later, &later, true),
        Err(AnchorError::Busy)
    );
    drop(permit);
    let retired = writer.prepare(&effect(), &later, &later, true)?;
    writer.reconcile(&retired.next, &retired.next)?;
    assert!(matches!(
        writer.reader().consume_publish(&checkpoint, &envelope),
        Err(AnchorError::Unreconciled)
    ));
    Ok(())
}

#[test]
fn partial_consumption_is_retained_and_cannot_be_reissued() -> TestResult {
    let home = Home::new()?;
    let (configured, envelope) = envelope("original")?;
    let mut writer = AnchorWriter::create(&home.0, configured.clone())?;
    let digest = publish_digest(&configured.namespace, &envelope)?;
    let checkpoint = commit(&mut writer, &digest)?;
    let path = home.0.join("effects").join(format!("{digest}.json"));
    super::super::file::options().open(&path)?;
    assert!(writer
        .reader()
        .consume_publish(&checkpoint, &envelope)
        .is_err());
    assert!(path.exists());
    assert_eq!(std::fs::read(&path)?, Vec::<u8>::new());
    drop(writer);
    let cold = AnchorReader::open(&home.0, configured)?;
    assert!(cold.consume_publish(&checkpoint, &envelope).is_err());
    assert!(path.exists());
    Ok(())
}

#[test]
fn historical_commit_cannot_be_erased_by_observing_both_databases_before() -> TestResult {
    let home = Home::new()?;
    let (configured, envelope) = envelope("original")?;
    let mut writer = AnchorWriter::create(&home.0, configured.clone())?;
    let before = writer.reader().checkpoint()?;
    let prepared = writer.prepare(
        &publish_digest(&configured.namespace, &envelope)?,
        &before,
        &before,
        false,
    )?;
    // Real cut between the independent history fsync and checkpoint rename.
    writer.reader().retain_checkpoint(&prepared)?;
    drop(writer);
    let mut cold = AnchorWriter::open(&home.0, configured)?;
    assert_eq!(
        cold.reconcile(&before, &before),
        Err(AnchorError::Unreconciled)
    );
    assert_eq!(cold.pending()?, Some(prepared.clone()));
    cold.reconcile(&prepared.next, &prepared.next)?;
    assert!(matches!(
        cold.reader().consume_publish(&prepared.next, &envelope)?,
        ConsumePublish::Fresh(_)
    ));
    Ok(())
}

#[test]
fn committed_refusal_is_independent_terminal_metadata_never_another_permit() -> TestResult {
    for reason in [
        PublishRefusal::SourceChanged,
        PublishRefusal::Expired,
        PublishRefusal::Cancelled,
        PublishRefusal::Closed,
    ] {
        let home = Home::new()?;
        let (configured, envelope) = envelope("negative-original")?;
        let mut writer = AnchorWriter::create(&home.0, configured.clone())?;
        let checkpoint = commit(
            &mut writer,
            &publish_digest(&configured.namespace, &envelope)?,
        )?;
        assert_eq!(
            writer
                .reader()
                .publication_refusal(&checkpoint, &envelope)?,
            None
        );
        let ConsumePublish::Fresh(permit) =
            writer.reader().consume_publish(&checkpoint, &envelope)?
        else {
            return Err("missing first permit".into());
        };
        let original = permit.condition().clone();
        assert_eq!(
            writer
                .reader()
                .publication_refusal(&checkpoint, &envelope)?,
            None
        );
        permit.record_committed_refusal(reason)?;
        permit.record_committed_refusal(reason)?;
        assert_eq!(
            writer
                .reader()
                .publication_refusal(&checkpoint, &envelope)?,
            Some(reason)
        );
        assert_eq!(
            writer.reader().inspect_publish(&checkpoint, &envelope)?,
            original
        );
        assert!(
            permit.check(&envelope).is_err(),
            "terminal refusal retires held permit"
        );
        let other = if reason == PublishRefusal::Closed {
            PublishRefusal::Cancelled
        } else {
            PublishRefusal::Closed
        };
        assert!(permit.record_committed_refusal(other).is_err());
        drop(permit);
        drop(writer);
        let reopened = AnchorWriter::open(&home.0, configured)?;
        assert_eq!(
            reopened
                .reader()
                .publication_refusal(&checkpoint, &envelope)?,
            Some(reason)
        );
        assert!(
            matches!(reopened.reader().consume_publish(&checkpoint, &envelope)?, ConsumePublish::Consumed(condition) if condition == original)
        );
    }
    Ok(())
}

#[test]
fn refusal_write_and_sync_failures_never_recreate_a_consumed_effect() -> TestResult {
    use super::super::faults::{self, Point};
    for point in [Point::Write, Point::FileSync, Point::DirectorySync] {
        let home = Home::new()?;
        let (configured, envelope) = envelope("negative-fsync")?;
        let mut writer = AnchorWriter::create(&home.0, configured.clone())?;
        let checkpoint = commit(
            &mut writer,
            &publish_digest(&configured.namespace, &envelope)?,
        )?;
        let ConsumePublish::Fresh(permit) =
            writer.reader().consume_publish(&checkpoint, &envelope)?
        else {
            return Err("missing first permit".into());
        };
        faults::fail(point);
        assert_eq!(
            permit.record_committed_refusal(PublishRefusal::Cancelled),
            Err(AnchorError::Unavailable)
        );
        let observed = writer
            .reader()
            .publication_refusal(&checkpoint, &envelope)?;
        assert_eq!(
            observed,
            if point == Point::DirectorySync {
                Some(PublishRefusal::Cancelled)
            } else {
                None
            }
        );
        assert!(matches!(
            writer.reader().consume_publish(&checkpoint, &envelope)?,
            ConsumePublish::Consumed(_)
        ));
        // The still-live original owner knows its prior transaction's result;
        // retrying that terminal write does not retry a publication.
        permit.record_committed_refusal(PublishRefusal::Cancelled)?;
        assert_eq!(
            writer
                .reader()
                .publication_refusal(&checkpoint, &envelope)?,
            Some(PublishRefusal::Cancelled)
        );
    }
    Ok(())
}
