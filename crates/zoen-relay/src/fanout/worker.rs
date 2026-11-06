//! Best-effort wake-only bus. It carries no frame, content or publish permit.
use super::*;
use tokio::sync::mpsc;
pub(super) struct WorkerHints {
    queue: mpsc::Sender<String>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for WorkerHints {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl WorkerHints {
    pub(super) async fn connect(url: &str, cell: &str) -> anyhow::Result<Self> {
        let client = async_nats::ConnectOptions::new()
            .name("zoen-worker-wakes")
            .connection_timeout(Duration::from_secs(5))
            .connect(url)
            .await?;
        let (queue, mut received) = mpsc::channel::<String>(256);
        let cell = cell.to_owned();
        let task = tokio::spawn(async move {
            while let Some(identity) = received.recv().await {
                if let Some(subject) = worker_subject(&cell, &identity) {
                    let _ = tokio::time::timeout(
                        Duration::from_secs(2),
                        client.publish(subject, Vec::new().into()),
                    )
                    .await;
                }
            }
        });
        Ok(Self { queue, task })
    }
    pub(super) fn wake(&self, identities: &[String]) {
        for identity in identities {
            let _ = self.queue.try_send(identity.clone());
        }
    }
}
