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
    _deadline: tokio::time::Instant,
) -> Result<String, client_core::CoreError> {
    if offline {
        eprintln!("offline cache export: remote catch-up was not checked");
    } else {
        // Startup already spent the command's wait budget. Check
        // its actual completion, without another wait or retry,
        // before enumerating a projection that may omit items.
        e.wait_until_settled(0).await?;
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
