use super::*;
use event_log::{Author, Signer};
use relay_admission::managed_images::{AnchorError, AnchorWriter, Deployment, Recovery};

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

#[tokio::test]
async fn cancelled_publisher_cannot_release_generation_while_its_batch_can_commit() {
    let directory = OwnedDirectory(std::env::temp_dir().join(types::new_id("managed-queue")));
    let mut writer = AnchorWriter::create(
        &directory.0,
        Deployment {
            namespace: "managed-queue".into(),
            witness: "11".repeat(32),
            source: "local-cell".into(),
            agent: "agent".into(),
            device: "device".into(),
        },
    )
    .expect("owned anchor");
    let reader = writer.reader();
    let current = reader.checkpoint().expect("genesis");
    let generation = Arc::new(
        reader
            .generation_guard(&current)
            .expect("current generation"),
    );
    let batcher = Arc::new(HeldBatch {
        entered: tokio::sync::Notify::new(),
        release: Semaphore::new(0),
    });
    let sequencer = Arc::new(Sequencer::new(batcher.clone()));
    let author = Author::root(Signer::generate());
    let envelope = Envelope::plain(&author.sign_event(
        "space",
        "message",
        0,
        None,
        types::EventBody::MessagePosted {
            message: "message".into(),
            text: "fixture".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    ));
    let caller = tokio::spawn({
        let sequencer = sequencer.clone();
        async move {
            sequencer
                .append_guarded(
                    envelope,
                    true,
                    Fence {
                        partition: crate::ownership::partition_of("space"),
                        owner: "test".into(),
                        token: 1,
                    },
                    Some(generation),
                )
                .await
        }
    });
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
        writer.prepare(&effect, &current, &current, true),
        Err(AnchorError::Busy),
        "a disconnected caller cannot retire a still-executing batch"
    );
    assert_eq!(sequencer.count.available_permits(), MAX_PENDING - 1);
    batcher.release.add_permits(1);
    let retired = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match writer.prepare(&effect, &current, &current, true) {
                Ok(prepared) => break prepared,
                Err(AnchorError::Busy) => tokio::task::yield_now().await,
                Err(error) => panic!("unexpected retirement error: {error}"),
            }
        }
    })
    .await
    .expect("completed worker releases generation");
    assert_eq!(retired.next.generation, current.generation + 1);
    assert_eq!(sequencer.count.available_permits(), MAX_PENDING);
    assert_eq!(
        writer
            .reconcile(&current, &current)
            .expect("abort unused retirement"),
        Recovery::BothBefore
    );
    assert!(reader.generation_guard(&current).is_ok());
}
