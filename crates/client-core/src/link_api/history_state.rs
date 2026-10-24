//! History state behind the unchanged native Client facade.

use super::{
    chunk_meta, history_commit, invalid, link, owner, t, valid_history_manifest, Client, CoreError,
    Duration, HistoryBox, HistoryDto, HistoryReceive, HistoryStatusDto, Op, Ordering, Reply,
    SecretVault, META_HISTORY, META_INITIAL_HISTORY_PENDING, META_PRIMARY,
};

impl Client {
    pub(super) fn history_status_impl(&self) -> Result<HistoryStatusDto, CoreError> {
        let _language = crate::i18n::enter(self.lang);
        let e = self.lock();
        let manifest = e
            .store
            .meta(META_HISTORY)?
            .map(|json| {
                serde_json::from_str::<HistoryBox>(&json)
                    .map_err(|_| invalid("Invalid retained history manifest."))
            })
            .transpose()?;
        if manifest
            .as_ref()
            .is_some_and(|value| !valid_history_manifest(value))
        {
            return Err(invalid("Invalid retained history manifest."));
        }
        let mut received = 0;
        if let Some(manifest) = &manifest {
            for n in 0..manifest.chunks.len() {
                if e.store.meta(&chunk_meta(n))?.is_some() {
                    received += 1;
                }
            }
        }
        Ok(HistoryStatusDto {
            initial_pending: e.store.meta(META_INITIAL_HISTORY_PENDING)?.is_some()
                || manifest.is_some(),
            manifest_received: manifest.is_some(),
            chunks_received: received,
            chunks_total: manifest.map_or(0, |value| value.chunks.len() as u32),
            receive_active: self.history_receive.active.load(Ordering::Acquire),
        })
    }

    pub(super) fn cancel_history_receive_impl(&self) {
        let _language = crate::i18n::enter(self.lang);
        self.history_receive.cancel();
    }

    pub(super) fn retire_link_pickup_impl(&self) {
        self.link_pickup.retire();
    }

    /// The history box: from the device database if an earlier try got it, else from the
    /// relay (immutable until its original expiry), kept until the bundle is in.
    pub(super) async fn history_manifest(
        &self,
        receive: &mut HistoryReceive,
        owner: &owner::LinkOwner,
    ) -> Result<HistoryBox, CoreError> {
        if let Some(json) = owner.with(self, |engine| Ok(engine.store.meta(META_HISTORY)?))? {
            let m: HistoryBox = serde_json::from_str(&json)
                .map_err(|_| invalid("Invalid retained history manifest."))?;
            if !valid_history_manifest(&m) {
                return Err(invalid("Invalid retained history manifest."));
            }
            return Ok(m);
        }
        let peer = owner.with(self, |e| {
            let primary = e.store.meta(META_PRIMARY)?.ok_or_else(|| {
                invalid(t(
                    "Este aparelho não foi vinculado.",
                    "This device wasn't linked.",
                ))
            })?;
            e.net
                .link
                .peers
                .get(&primary)
                .cloned()
                .ok_or_else(|| invalid("peer"))
        })?;
        let secret = peer.secret().ok_or_else(|| invalid("peer"))?;
        let id = link::box_id(&secret, "history");
        let sealed = loop {
            match receive
                .wait(owner.request(self, Op::FetchLink { id: id.clone() }))
                .await?
            {
                Reply::Link(Some(b)) => break b,
                Reply::Link(None) => {}
                _ => return Err(invalid("the relay answered something else")),
            }
            receive
                .wait(async {
                    tokio::time::sleep(Duration::from_millis(700)).await;
                    Ok(())
                })
                .await?;
        };
        if sealed.len() > 65_536 {
            return Err(invalid("The history manifest is too large."));
        }
        let sk =
            hex::decode(&peer.my_hpke_secret).map_err(|_| invalid("Invalid retained peer key."))?;
        let plain = link::open(&sk, &secret, "history", &sealed)
            .ok_or_else(|| invalid(t("O histórico não abriu.", "The history didn't open.")))?;
        let m: HistoryBox = serde_json::from_slice(&plain).map_err(|e| invalid(e.to_string()))?;
        if !valid_history_manifest(&m) {
            return Err(invalid("Invalid history manifest."));
        }
        receive.check()?;
        let encoded = serde_json::to_string(&m).map_err(|error| invalid(error.to_string()))?;
        owner.with(self, |engine| {
            receive.check()?;
            engine.store.set_meta(META_HISTORY, &encoded)?;
            Ok(())
        })?;
        Ok(m)
    }

    /// Import and completion marker share the existing SQLite transaction.
    pub(super) fn commit_initial_history_owned(
        &self,
        owner: &owner::LinkOwner,
        events: Vec<types::Event>,
        chunks: u32,
        resumed: u32,
        receive: &HistoryReceive,
    ) -> Result<HistoryDto, CoreError> {
        owner.with(self, |engine| {
            history_commit::commit(engine, events, chunks, resumed, receive)
        })
    }

    // Local import fixtures exercise the same transaction without forging a Net.
    #[cfg(test)]
    pub(super) fn commit_initial_history(
        &self,
        events: Vec<types::Event>,
        chunks: u32,
        resumed: u32,
        receive: &HistoryReceive,
    ) -> Result<HistoryDto, CoreError> {
        history_commit::commit(&mut self.lock(), events, chunks, resumed, receive)
    }
}
