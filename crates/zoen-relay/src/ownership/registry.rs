//! Membership changes serialize at one key; ordinary heartbeats conflict only
//! with cleanup of their own key. Cleanup commits even when admission cannot.

use foundationdb::{
    options::StreamingMode,
    tuple::{pack, unpack, Subspace},
    FdbBindingError, KeySelector, RangeOption, Transaction,
};

use super::{corrupt, MAX_NODES};

async fn entries(
    trx: &Transaction,
    nodes: &Subspace,
    after: Option<Vec<u8>>,
) -> Result<(Vec<(Vec<u8>, Vec<u8>)>, bool), FdbBindingError> {
    let mut opt = RangeOption::from(nodes.range());
    opt.mode = StreamingMode::WantAll;
    if let Some(after) = after {
        opt.begin = KeySelector::first_greater_than(after);
    }
    let mut rows = Vec::new();
    let mut iteration = 1;
    loop {
        opt.limit = Some(MAX_NODES + 1 - rows.len());
        let page = trx.get_range(&opt, iteration, true).await?;
        let more = page.more();
        rows.extend(
            page.iter()
                .map(|kv| (kv.key().to_vec(), kv.value().to_vec())),
        );
        if !more || rows.len() == MAX_NODES + 1 {
            return Ok((rows, more));
        }
        let last = rows
            .last()
            .ok_or_else(|| corrupt("empty ownership registry page"))?;
        opt.begin = KeySelector::first_greater_than(last.0.clone());
        iteration += 1;
    }
}

async fn fence_membership(
    trx: &Transaction,
    root: &Subspace,
    version: i64,
) -> Result<(), FdbBindingError> {
    let guard = root.pack(&("ownership", "registry-admission"));
    trx.get(&guard, false).await?;
    trx.set(&guard, &pack(&version));
    Ok(())
}

/// Renews/adopts a node and returns the bounded live roster. `None` must still
/// be committed: an oversized registry's cleanup advances one bounded page.
pub async fn register_in(
    trx: &Transaction,
    root: &Subspace,
    node: &str,
    version: i64,
    until: i64,
) -> Result<Option<Vec<String>>, FdbBindingError> {
    let nodes = root.subspace(&("ownership", "nodes"));
    let own = nodes.pack(&node);
    // A concurrent cleanup cannot remove this heartbeat after it renews.
    trx.get(&own, false).await?;
    let (mut rows, mut more) = entries(trx, &nodes, None).await?;
    let cursor = root.pack(&("ownership", "registry-cleanup"));
    if rows.len() > MAX_NODES {
        fence_membership(trx, root, version).await?;
        let after = trx.get(&cursor, false).await?;
        if let Some(after) = after {
            let page = entries(trx, &nodes, Some(after.to_vec())).await?;
            rows = page.0;
            more = page.1;
        }
        for (key, value) in &rows {
            let expiry: i64 = unpack(value).map_err(|e| corrupt(e.to_string()))?;
            if expiry <= version {
                trx.get(key, false).await?;
                trx.clear(key);
            }
        }
        if more {
            let last = rows
                .last()
                .ok_or_else(|| corrupt("empty ownership registry page"))?;
            trx.set(&cursor, &last.0);
        } else {
            trx.clear(&cursor);
        }
        return Ok(None);
    }
    let mut live = Vec::new();
    let mut expired = Vec::new();
    for (key, value) in rows {
        let who: String = nodes.unpack(&key).map_err(|e| corrupt(e.to_string()))?;
        let expiry: i64 = unpack(&value).map_err(|e| corrupt(e.to_string()))?;
        if expiry > version {
            live.push(who);
        } else {
            expired.push(key);
        }
    }
    let joining = !live.iter().any(|who| who == node);
    if joining && live.len() == MAX_NODES {
        return Ok(None);
    }
    if joining || !expired.is_empty() {
        fence_membership(trx, root, version).await?;
        for key in expired {
            trx.get(&key, false).await?;
            trx.clear(&key);
        }
        trx.clear(&cursor);
    }
    if joining {
        live.push(node.to_string());
    }
    trx.set(&own, &pack(&until));
    Ok(Some(live))
}
