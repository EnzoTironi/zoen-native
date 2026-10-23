use super::*;

pub(super) async fn collect_presence(
    asks: impl IntoIterator<Item = impl std::future::Future<Output = Option<String>>>,
) -> Vec<String> {
    async fn indexed<F: std::future::Future<Output = Option<String>>>(
        (index, ask): (usize, F),
    ) -> (usize, Option<String>) {
        (index, ask.await)
    }
    let mut asks = asks.into_iter().enumerate();
    let mut pending = futures_util::stream::FuturesUnordered::new();
    for ask in asks.by_ref().take(PRESENCE_CONCURRENCY) {
        pending.push(indexed(ask));
    }
    let mut online = Vec::new();
    // Keep completed probes even if an earlier request stalls. Dropping this stream at
    // the overall deadline cancels in-flight requests without spawning background work.
    let _ = tokio::time::timeout(PRESENCE_LOOKUP_TIMEOUT, async {
        while let Some((index, id)) = pending.next().await {
            if let Some(id) = id {
                online.push((index, id));
            }
            if let Some(ask) = asks.next() {
                pending.push(indexed(ask));
            }
        }
    })
    .await;
    online.sort_unstable_by_key(|(index, _)| *index);
    online.into_iter().map(|(_, id)| id).collect()
}
