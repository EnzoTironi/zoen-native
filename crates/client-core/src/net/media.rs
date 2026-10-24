//! Encrypted media HTTP transfer and existing bounded retry ownership.

use super::{
    now_ms, tracing_like, Arc, CoreListener, Duration, EngineLease, MediaDownloadRetries,
    RecoveryRetries,
};

/// Uploads queued encrypted copies and downloads the ones the chats need, retrying with
/// backoff (a peer's event can arrive before their upload finishes).
pub(super) async fn media_worker(
    engine: EngineLease,
    listener: Option<Arc<dyn CoreListener>>,
    base: String,
    relay: String,
    key: event_log::Signer,
    kick: Arc<tokio::sync::Notify>,
) {
    macro_rules! engine {
        ($work:expr) => {
            match engine.with($work) {
                Some(value) => value,
                None => return,
            }
        };
    }

    let Ok(http) = crate::https::client()
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        tracing_like("media: no HTTP client");
        return;
    };
    let mut download_retries = MediaDownloadRetries::default();
    let mut recovery_retries = RecoveryRetries::default();
    loop {
        // ── up ──
        // sync_idle waits until this queue is empty, so a full batch is drained
        // before the download pass and the idle sleep.
        let uploads = engine!(|engine| engine.pending_uploads(8));
        let batch = uploads.len();
        let mut upload_failed = false;
        for (sha, bytes) in uploads {
            let res =
                crate::blob_transport::put(&http, &base, &relay, &key, &sha, bytes, now_ms()).await;
            match res {
                Ok(receipt) => engine!(|engine| engine.upload_done(receipt.blob())),
                Err(crate::blob_transport::BlobUploadError::Refused(status @ (400 | 413))) => {
                    // The relay will never take it (corrupt or too big): don't loop forever.
                    tracing_like(&format!(
                        "media: upload {} refused ({})",
                        &sha[..12],
                        status
                    ));
                    match engine!(|engine| engine.upload_refused(&sha)) {
                        Ok(spaces) => {
                            if let Some(listener) = &listener {
                                if !spaces.is_empty() {
                                    listener.on_change(spaces);
                                }
                            }
                        }
                        Err(error) => {
                            tracing_like(&format!("media: refusal storage failed: {error}"));
                            upload_failed = true;
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing_like(&format!("media: upload failed: {e}"));
                    upload_failed = true;
                    break;
                }
            }
        }
        if batch == 8 && !upload_failed {
            continue;
        }

        // Recovery uses a separate bounded download. Only ciphertext is cached, and
        // its content address must match the signed commit before MLS sees any bytes.
        let wanted = engine!(|engine| engine.wanted_recovery_contexts());
        if let Some(reference) = recovery_retries.next(wanted, tokio::time::Instant::now()) {
            let bytes = bounded_blob(
                &http,
                &format!("{base}/v1/blobs/{}", reference.blob),
                event_log::recovery::MAX_RECOVERY_BLOB_BYTES,
            )
            .await;
            if match bytes {
                Some(bytes) => {
                    engine!(|engine| engine.recovery_context_arrived(&reference, &bytes))
                }
                None => false,
            } {
                recovery_retries.complete(&reference);
            } else {
                recovery_retries.defer(reference, tokio::time::Instant::now());
            }
        }

        // ── down ──
        let mut foreground = engine!(|engine| engine.take_foreground_file_attempts());
        let wanted = engine!(|engine| engine.wanted_media());
        let now = tokio::time::Instant::now();
        let mut changed: Vec<String> = Vec::new();
        for w in wanted {
            // A canceled historical open or changed membership must not start
            // another fetch from a stale snapshot of the demand list.
            if !engine!(|engine| engine.wanted_media().contains(&w)) {
                continue;
            }
            if !download_retries.ready(&w.blob, foreground.remove(&w), now) {
                continue;
            }
            let url = format!("{base}/v1/blobs/{}", w.blob);
            let got =
                bounded_blob(&http, &url, usize::try_from(w.sealed_limit()).unwrap_or(0)).await;
            let ok =
                match got {
                    Some(bytes) => engine!(|engine| engine.wanted_media().contains(&w)
                        && engine.media_arrived(&w, &bytes)),
                    None => false,
                };
            if ok {
                download_retries.completed(&w.blob);
                if let Some(id) = &w.profile {
                    if let Some(l) = &listener {
                        l.on_profile_changed(id.clone());
                    }
                } else if !changed.contains(&w.space) {
                    changed.push(w.space.clone());
                }
            } else {
                download_retries.failed(&w.blob, now);
            }
        }
        if !changed.is_empty() {
            if let Some(l) = &listener {
                l.on_change(changed);
            }
        }

        tokio::select! {
            () = kick.notified() => {}
            () = tokio::time::sleep(Duration::from_secs(5)) => {}
        }
    }
}

pub(super) async fn bounded_blob(
    http: &reqwest::Client,
    url: &str,
    limit: usize,
) -> Option<Vec<u8>> {
    let mut response = http.get(url).send().await.ok()?;
    if !response.status().is_success()
        || response.content_length().is_some_and(|n| n > limit as u64)
    {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    (!bytes.is_empty()).then_some(bytes)
}
