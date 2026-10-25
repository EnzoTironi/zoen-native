use super::*;
use event_log::{Author, Signer};
use relay_admission::managed_images::{
    publish_digest, AnchorError, AnchorWriter, Checkpoint, Deployment, Recovery,
};

struct HeldBatch {
    entered: tokio::sync::Notify,
    release: Semaphore,
}

#[async_trait]
impl Batcher for HeldBatch {
    type State = ();
    async fn append_batch(&self, _: &str, batch: &[Pending], _: &mut ()) -> Applied {
        self.entered.notify_one();
        self.release
            .acquire()
            .await
            .expect("test batch gate")
            .forget();
        Applied {
            results: batch
                .iter()
                .map(|_| Err(Reject::no("fixture result")))
                .collect(),
            attempts: 1,
        }
    }
}

struct OwnedDirectory(std::path::PathBuf);
impl Drop for OwnedDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Case {
    writer: AnchorWriter,
    envelope: Envelope,
    checkpoint: Checkpoint,
    _directory: OwnedDirectory,
}
impl Case {
    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let directory = OwnedDirectory(std::env::temp_dir().join(types::new_id("managed-queue")?));
        let author = Author::device(&Signer::generate()?, Signer::generate()?);
        let envelope = Envelope::sealed(
            &author,
            "space",
            "message",
            1,
            None,
            protocol::Sealed::new(protocol::SealedKind::Application, 1, vec![3; 32]),
        )?;
        let deployment = Deployment {
            namespace: "managed-queue".into(),
            witness: "11".repeat(32),
            source: "local-cell".into(),
            agent: author.identity.clone(),
            device: author.device.clone().expect("certified device"),
        };
        let mut writer =
            AnchorWriter::create(&directory.0, deployment.clone()).expect("owned anchor");
        let genesis = writer.reader().checkpoint().expect("genesis");
        let digest = publish_digest(&deployment.namespace, &envelope).expect("original wire");
        let prepared = writer
            .prepare(&digest, &genesis, &genesis, false)
            .expect("original checkpoint");
        assert_eq!(
            writer.reconcile(&prepared.next, &prepared.next).unwrap(),
            Recovery::BothAfter
        );
        Ok(Self {
            writer,
            envelope,
            checkpoint: prepared.next,
            _directory: directory,
        })
    }
    fn fence(&self) -> Fence {
        Fence {
            partition: crate::ownership::partition_of("space"),
            owner: "test".into(),
            token: 1,
        }
    }
    fn admit(&self, sequencer: &Sequencer<HeldBatch>) -> Result<ManagedAdmission, Reject> {
        sequencer.admit_managed(
            self.envelope.clone(),
            true,
            self.fence(),
            &self.writer.reader(),
            &self.checkpoint,
        )
    }
}
fn batcher() -> Arc<HeldBatch> {
    Arc::new(HeldBatch {
        entered: tokio::sync::Notify::new(),
        release: Semaphore::new(0),
    })
}

#[tokio::test]
async fn cancelled_publisher_cannot_release_generation_while_its_batch_can_commit(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut case = Case::new()?;
    let batcher = batcher();
    let sequencer = Arc::new(Sequencer::new(batcher.clone()));
    let ManagedAdmission::Queued(queued) = case.admit(&sequencer).expect("admitted once") else {
        panic!("original effect must be fresh");
    };
    let caller = tokio::spawn(queued.append());
    tokio::time::timeout(Duration::from_secs(3), batcher.entered.notified())
        .await
        .expect("batch entered");
    caller.abort();
    assert!(caller
        .await
        .expect_err("cancelled publisher")
        .is_cancelled());
    let effect = "22".repeat(32);
    assert_eq!(
        case.writer
            .prepare(&effect, &case.checkpoint, &case.checkpoint, true),
        Err(AnchorError::Busy)
    );
    assert_eq!(sequencer.count.available_permits(), MAX_PENDING - 1);
    batcher.release.add_permits(1);
    let retired = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match case
                .writer
                .prepare(&effect, &case.checkpoint, &case.checkpoint, true)
            {
                Ok(prepared) => break prepared,
                Err(AnchorError::Busy) => tokio::task::yield_now().await,
                Err(error) => panic!("unexpected retirement error: {error}"),
            }
        }
    })
    .await
    .expect("completed worker releases generation");
    assert_eq!(retired.next.generation, case.checkpoint.generation + 1);
    assert_eq!(sequencer.count.available_permits(), MAX_PENDING);
    assert_eq!(
        case.writer
            .reconcile(&case.checkpoint, &case.checkpoint)
            .unwrap(),
        Recovery::BothBefore
    );
    Ok(())
}

#[tokio::test]
async fn saturated_count_bytes_or_space_slots_do_not_consume_the_original_effect(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for limit in ["count", "bytes", "slots"] {
        let case = Case::new()?;
        let mut sequencer = Sequencer::new(batcher());
        let mut reservations = Vec::new();
        match limit {
            "count" => sequencer.count = Arc::new(Semaphore::new(0)),
            "bytes" => sequencer.bytes = Arc::new(Semaphore::new(0)),
            "slots" => {
                for _ in 0..SPACE_QUEUE {
                    reservations.push(
                        sequencer
                            .reserve(case.envelope.clone(), true, case.fence())
                            .expect("space slot"),
                    );
                }
            }
            _ => unreachable!("declared fixture limit"),
        }
        assert!(
            case.admit(&sequencer).is_err(),
            "{limit} must refuse before consumption"
        );
        drop(reservations);
        sequencer.count = Arc::new(Semaphore::new(MAX_PENDING));
        sequencer.bytes = Arc::new(Semaphore::new(MAX_PENDING_BYTES));
        assert!(
            matches!(
                case.admit(&sequencer).expect("capacity restored"),
                ManagedAdmission::Queued(_)
            ),
            "{limit} refusal must leave the exact original effect unconsumed"
        );
    }
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn reserved_slot_keeps_the_worker_alive_across_idle_until_the_original_is_enqueued(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let case = Case::new()?;
    let batcher = batcher();
    let mut sequencer = Sequencer::new(batcher.clone());
    sequencer.idle = Duration::from_secs(1);
    let ManagedAdmission::Queued(queued) = case.admit(&sequencer).expect("reserved original")
    else {
        panic!("original must be fresh");
    };
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(2)).await;
    tokio::task::yield_now().await;
    assert_eq!(
        sequencer.active().unwrap(),
        1,
        "idle cannot close a reserved sender's receiver"
    );
    let caller = tokio::spawn(queued.append());
    batcher.entered.notified().await;
    batcher.release.add_permits(1);
    let (result, stats) = caller.await.expect("worker result");
    assert_eq!(result.unwrap_err().reason, "fixture result");
    assert_eq!(stats.size, 1);
    assert_eq!(sequencer.count.available_permits(), MAX_PENDING);
    Ok(())
}
