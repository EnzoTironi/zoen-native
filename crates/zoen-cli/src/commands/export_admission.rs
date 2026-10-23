//! The real export branch: completion admission precedes enumeration and writes.
use crate::*;

#[cfg(test)]
mod test_fixture;
#[cfg(test)]
mod tests;

pub(super) async fn export_all(
    e: &Client,
    space: String,
    dir: PathBuf,
    timeout: u64,
    offline: bool,
    deadline: tokio::time::Instant,
) -> Result<String, client_core::CoreError> {
    if offline {
        eprintln!("offline cache export: remote catch-up was not checked");
    } else {
        // Startup may have finished early. MLS can acquire new work before this
        // observation; wait only within the original command's remaining window.
        // This is a completion observation before enumeration, not an atomic
        // snapshot: later events can still arrive while files are being written.
        let remaining = super::sync_budget::remaining_ms(deadline);
        if remaining == 0 {
            return Err(deadline_error());
        }
        e.wait_until_settled(remaining).await?;
        // The existing Core poll can resume after its deadline. Such a late ACK
        // must not admit a new export after this command's original wait window.
        if tokio::time::Instant::now() >= deadline {
            return Err(deadline_error());
        }
    }
    let (mut n, mut bytes, mut missing) = (0, 0, 0);
    for i in e
        .items()
        .into_iter()
        .filter(|i| i.space_id == space && i.kind_id == "page")
    {
        if !ready(e, &i.id, timeout).await {
            missing += 1;
            continue;
        }
        let md = e.page_markdown(i.id.clone()).unwrap_or_else(|err| die(err));
        let name = if i.path.is_empty() {
            format!("{}.md", i.id)
        } else {
            i.path.clone()
        };
        write_file(&dir.join(name), md.as_bytes());
        n += 1;
        bytes += md.len();
    }
    Ok(format!(
        "{n} pages, {bytes} bytes, {missing} still arriving"
    ))
}

fn deadline_error() -> client_core::CoreError {
    client_core::CoreError::Invalid {
        reason: "Sync did not finish before the original export deadline; queued work is saved for retry.".into(),
    }
}
