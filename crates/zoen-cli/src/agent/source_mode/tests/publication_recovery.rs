//! The real Client must recover a committed publication after both receipts are lost.
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnect_replays_the_original_publication_once_after_lost_receipts(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = relay(Mode::LostPublicationReceipt).await;
    let mut f = Fixture::new(&server.url)?;
    let prefix = f.history()?;
    let prefix_len = prefix.len();
    server
        .histories
        .lock()
        .unwrap()
        .insert(f.space.clone(), prefix);
    online(&f.client, true).await;
    let queued = f
        .client
        .send_message(f.space.clone(), "Recover this message once.".into())?;
    let settled = f.client.wait_until_settled(3000).await;
    let history = server.histories.lock().unwrap()[&f.space].clone();
    let publications = server
        .calls
        .lock()
        .unwrap()
        .iter()
        .filter(|call| **call == "event_publish")
        .count();
    assert!(
        publications >= 2,
        "the unconfirmed intent must cross a second socket"
    );
    assert_eq!(
        history.len(),
        prefix_len + 1,
        "a retry must reuse the original sequence"
    );
    settled?;
    assert_eq!(f.client.connection().pending, 0);
    let timeline = f.client.timeline(f.space.clone())?;
    assert_eq!(
        timeline
            .iter()
            .filter(|entry| entry.id == queued.id)
            .count(),
        1
    );
    let accepted = timeline
        .iter()
        .find(|entry| entry.id == queued.id)
        .ok_or("accepted message")?;
    assert_eq!(accepted.delivery, client_core::Delivery::Sent);
    assert_eq!(accepted.seq, history.last().ok_or("relay head")?.seq);
    f.reopen();
    online(&f.client, true).await;
    assert_eq!(
        f.client
            .timeline(f.space.clone())?
            .iter()
            .filter(|entry| entry.id == queued.id)
            .count(),
        1
    );
    assert_eq!(
        server.histories.lock().unwrap()[&f.space].len(),
        prefix_len + 1
    );
    Ok(())
}
