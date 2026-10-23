//! History state behind the unchanged native Client facade.

use super::*;

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
    /// relay (one-time), kept until the bundle is in.
    pub(super) async fn history_manifest(
        &self,
        receive: &mut HistoryReceive,
    ) -> Result<HistoryBox, CoreError> {
        if let Some(json) = self.lock().store.meta(META_HISTORY)? {
            let m: HistoryBox = serde_json::from_str(&json)
                .map_err(|_| invalid("Invalid retained history manifest."))?;
            if !valid_history_manifest(&m) {
                return Err(invalid("Invalid retained history manifest."));
            }
            return Ok(m);
        }
        let peer = {
            let e = self.lock();
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
                .ok_or_else(|| invalid("peer"))?
        };
        let secret = peer.secret().ok_or_else(|| invalid("peer"))?;
        let id = link::box_id(&secret, "history");
        let sealed = loop {
            match receive
                .wait(self.request(Op::FetchLink { id: id.clone() }))
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
        let sk = hex::decode(&peer.my_hpke_secret).unwrap_or_default();
        let plain = link::open(&sk, &secret, "history", &sealed)
            .ok_or_else(|| invalid(t("O histórico não abriu.", "The history didn't open.")))?;
        let m: HistoryBox = serde_json::from_slice(&plain).map_err(|e| invalid(e.to_string()))?;
        if !valid_history_manifest(&m) {
            return Err(invalid("Invalid history manifest."));
        }
        receive.check()?;
        self.lock()
            .store
            .set_meta(META_HISTORY, &serde_json::to_string(&m).unwrap_or_default())?;
        Ok(m)
    }

    /// Import and completion marker share one SQLite commit. A missing relay record
    /// is pending work, never an empty successful history transfer.
    pub(super) fn commit_initial_history(
        &self,
        events: Vec<types::Event>,
        chunks: u32,
        resumed: u32,
        receive: &HistoryReceive,
    ) -> Result<HistoryDto, CoreError> {
        receive.check()?;
        let mut e = self.lock();
        let mut positions = BTreeSet::new();
        let mut opened = Vec::new();
        for event in events {
            receive.check()?;
            if !positions.insert((event.space.clone(), event.seq)) {
                return Err(invalid("The history repeats a relay position."));
            }
            let local = e
                .logs
                .get(&event.space)
                .and_then(|log| {
                    usize::try_from(event.seq)
                        .ok()
                        .and_then(|index| log.events().get(index))
                })
                .ok_or_else(|| {
                    invalid(t(
                        "O histórico aguarda os registros verificados do servidor.",
                        "History is waiting for the verified relay records.",
                    ))
                })?;
            if local.sealed_wire.is_some() {
                // A retry after a former partial import must be the exact same opened event.
                let mut anchored = event;
                anchored.prev = local.prev.clone();
                anchored.hash = local.hash.clone();
                if &anchored != local {
                    return Err(invalid("An opened history entry does not match."));
                }
                continue;
            }
            let event = link::opened_history_event(local, event).ok_or_else(|| {
                invalid("A history entry does not match the signed relay record.")
            })?;
            opened.push(event);
        }
        let receipt = HistoryDto {
            messages: opened.len() as u32,
            chunks,
            resumed,
        };
        {
            let tx = e
                .store
                .conn()
                .unchecked_transaction()
                .map_err(|err| CoreError::Storage {
                    message: err.to_string(),
                })?;
            for event in opened {
                receive.check()?;
                if !e.store.replace_event(&event)? {
                    return Err(invalid("A history relay record disappeared."));
                }
            }
            e.store.set_meta(
                META_HISTORY_COMPLETE,
                &serde_json::to_string(&receipt).map_err(|err| invalid(err.to_string()))?,
            )?;
            for n in 0..chunks {
                e.store.meta_delete(&chunk_meta(n as usize))?;
            }
            e.store.meta_delete(META_HISTORY)?;
            e.store.meta_delete(META_INITIAL_HISTORY_PENDING)?;
            receive.check()?;
            tx.commit().map_err(|err| CoreError::Storage {
                message: err.to_string(),
            })?;
        }
        e.reload()?;
        Ok(receipt)
    }
}
