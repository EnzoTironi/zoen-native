//! Transfer behind the unchanged native Client facade.

use super::*;

impl Client {
    pub(super) async fn link_progress_impl(&self, device_id: String) -> Vec<u32> {
        let (done, total) = self.lock().link_progress(&device_id);
        vec![done, total]
    }

    pub(super) async fn send_history_impl(
        &self,
        device_id: String,
        listener: Option<Arc<dyn TransferListener>>,
    ) -> Result<u32, CoreError> {
        let owner = owner::LinkOwner::capture(self)?;
        let (bundle, peer) = owner.with(self, |e| {
            let peer = e.net.link.peers.get(&device_id).cloned().ok_or_else(|| {
                invalid(t(
                    "Aparelho não vinculado aqui.",
                    "Not a device linked here.",
                ))
            })?;
            Ok((e.history_bundle(), peer))
        })?;
        let plain = serde_json::to_vec(&bundle).map_err(|e| invalid(e.to_string()))?;
        let key = link::random::<32>();
        let transfer = hex::encode(link::random::<32>());
        let base = owner.relay_base(self)?;
        let client = http()?;
        let pieces: Vec<&[u8]> = plain.chunks(chunk_size()).collect();
        let total = pieces.len() as u32;
        let mut hashes = Vec::new();
        for (n, piece) in pieces.iter().enumerate() {
            let sealed = link::seal_chunk(&key, n as u32, piece);
            let sha = event_log::sha256_hex(&sealed);
            let (device, ts, sig) = owner.signed_headers(self, |ts| {
                transfer_message("put", &transfer, &n.to_string(), &sha, ts)
            })?;
            let r = client
                .put(format!("{base}/v1/transfer/{transfer}/{n}"))
                .header("x-zoen-device", device)
                .header("x-zoen-ts", ts)
                .header("x-zoen-sig", sig)
                .body(sealed)
                .send()
                .await
                .map_err(|e| invalid(e.to_string()))?;
            if !r.status().is_success() {
                return Err(invalid(format!("history upload: {}", r.status())));
            }
            owner.with(self, |_| Ok(()))?;
            hashes.push(sha);
            if let Some(l) = &listener {
                l.on_progress(n as u32 + 1, total);
            }
        }
        let manifest = HistoryBox {
            transfer,
            key: hex::encode(key),
            chunks: hashes,
        };
        let secret = peer.secret().ok_or_else(|| invalid("peer"))?;
        let to =
            hex::decode(&peer.their_hpke).map_err(|_| invalid("Invalid retained peer key."))?;
        let sealed = link::seal(
            &to,
            &secret,
            "history",
            &serde_json::to_vec(&manifest).map_err(|error| invalid(error.to_string()))?,
        )
        .ok_or_else(|| invalid("sealing"))?;
        match owner
            .request(
                self,
                Op::DeliverLink {
                    id: link::box_id(&secret, "history"),
                    sealed,
                    device: None,
                },
            )
            .await?
        {
            Reply::Done => {
                // A best-effort wake only after the manifest exists durably. The
                // destination's pending marker + foreground/reconnect resume are fallback.
                owner.with(self, |engine| {
                    engine.announce_initial_history(&device_id);
                    Ok(())
                })?;
                Ok(bundle.events.len() as u32)
            }
            _ => Err(invalid("the relay answered something else")),
        }
    }

    pub(super) async fn receive_history_impl(
        &self,
        timeout_ms: u64,
        listener: Option<Arc<dyn TransferListener>>,
    ) -> Result<HistoryDto, CoreError> {
        crate::i18n::scope(self.lang, async {
            let mut receive = self.history_receive.begin(timeout_ms)?;
            let starting_owner = {
                let e = self.lock();
                let starting_owner = directory::DirectoryOwner::capture(&e)?;
                if e.store.meta(META_INITIAL_HISTORY_PENDING)?.is_none() {
                    if let Some(receipt) = e.store.meta(META_HISTORY_COMPLETE)? {
                        return serde_json::from_str(&receipt)
                            .map_err(|_| invalid("Invalid history receipt."));
                    }
                }
                if e.store.meta(META_PRIMARY)?.is_none() {
                    return Err(invalid("This device was not linked."));
                }
                starting_owner
            };
            let owner = owner::LinkOwner::capture(self)?;
            owner.with(self, |engine| {
                starting_owner.check(engine)?;
                // Preserve the durable retry marker before fetching the one-time box.
                engine.store.set_meta(META_INITIAL_HISTORY_PENDING, "1")?;
                Ok(())
            })?;
            let manifest = self.history_manifest(&mut receive, &owner).await?;
            let key =
                secret32(hex::decode(&manifest.key).ok()).ok_or_else(|| invalid("history"))?;
            let base = owner.relay_base(self)?;
            let client = http()?;
            let total = manifest.chunks.len();
            let stop_after: Option<usize> = std::env::var("ZOEN_TRANSFER_STOP_AFTER")
                .ok()
                .and_then(|v| v.parse().ok());
            let mut resumed = 0;
            let mut fetched = 0;
            let b64 = base64::engine::general_purpose::STANDARD;
            for (n, sha) in manifest.chunks.iter().enumerate() {
                receive.check()?;
                if let Some(retained) =
                    owner.with(self, |engine| Ok(engine.store.meta(&chunk_meta(n))?))?
                {
                    if b64.decode(retained).ok().is_some_and(|bytes| {
                        bytes.len() <= MAX_HISTORY_CHUNK && event_log::sha256_hex(&bytes) == *sha
                    }) {
                        resumed += 1;
                        continue;
                    }
                    return Err(invalid("A retained history chunk does not match."));
                }
                if stop_after.is_some_and(|s| fetched >= s) {
                    return Err(invalid(format!("history stopped at {}/{total}", n)));
                }
                let mut r = receive
                    .wait(async {
                        client
                            .get(format!("{base}/v1/transfer/{}/{n}", manifest.transfer))
                            .send()
                            .await
                            .map_err(|e| invalid(e.to_string()))
                    })
                    .await?;
                if !r.status().is_success() {
                    return Err(invalid(format!("history download: {}", r.status())));
                }
                if r.content_length()
                    .is_some_and(|length| length > MAX_HISTORY_CHUNK as u64)
                {
                    return Err(invalid("A history chunk is too large."));
                }
                let mut bytes = Vec::new();
                while let Some(part) = receive
                    .wait(async { r.chunk().await.map_err(|e| invalid(e.to_string())) })
                    .await?
                {
                    if bytes.len().saturating_add(part.len()) > MAX_HISTORY_CHUNK {
                        return Err(invalid("A history chunk is too large."));
                    }
                    bytes.extend_from_slice(&part);
                }
                if event_log::sha256_hex(&bytes) != *sha {
                    return Err(invalid("a history chunk doesn't match"));
                }
                receive.check()?;
                owner.with(self, |engine| {
                    receive.check()?;
                    engine.store.set_meta(&chunk_meta(n), &b64.encode(&bytes))?;
                    Ok(())
                })?;
                fetched += 1;
                if let Some(l) = &listener {
                    l.on_progress((n + 1) as u32, total as u32);
                }
            }
            let mut plain = Vec::new();
            for n in 0..total {
                receive.check()?;
                let sealed = owner
                    .with(self, |engine| Ok(engine.store.meta(&chunk_meta(n))?))?
                    .and_then(|s| b64.decode(s).ok())
                    .ok_or_else(|| invalid("history chunk"))?;
                if sealed.len() > MAX_HISTORY_CHUNK
                    || event_log::sha256_hex(&sealed) != manifest.chunks[n]
                {
                    return Err(invalid("A retained history chunk does not match."));
                }
                let opened = link::open_chunk(&key, n as u32, &sealed)
                    .ok_or_else(|| invalid("A history chunk does not open."))?;
                if plain.len().saturating_add(opened.len()) > MAX_HISTORY_BYTES {
                    return Err(invalid("The history bundle is too large."));
                }
                plain.extend(opened);
            }
            let bundle: link::Bundle =
                serde_json::from_slice(&plain).map_err(|e| invalid(e.to_string()))?;
            if bundle.events.len() > MAX_HISTORY_EVENTS {
                return Err(invalid("The history bundle has too many events."));
            }
            // Receiving the box does not mean the authenticated relay chain is already here.
            while !self.connection().synced {
                owner.with(self, |_| Ok(()))?;
                receive
                    .wait(async {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        Ok(())
                    })
                    .await?;
            }
            let receipt = self.commit_initial_history_owned(
                &owner,
                bundle.events,
                total as u32,
                resumed,
                &receive,
            )?;
            // Local completion was committed before this best-effort relay cleanup.
            if let Ok((device, ts, sig)) = owner.signed_headers(self, |ts| {
                transfer_message("delete", &manifest.transfer, "", "", ts)
            }) {
                let _ = receive
                    .wait(async {
                        client
                            .delete(format!("{base}/v1/transfer/{}", manifest.transfer))
                            .header("x-zoen-device", device)
                            .header("x-zoen-ts", ts)
                            .header("x-zoen-sig", sig)
                            .send()
                            .await
                            .map_err(|error| invalid(error.to_string()))
                    })
                    .await;
            }
            Ok(receipt)
        })
        .await
    }

    pub(super) async fn load_older_impl(
        &self,
        space_id: String,
        timeout_ms: u64,
    ) -> Result<OlderDto, CoreError> {
        let Some(req) = self.lock().request_page(&space_id)? else {
            return Ok(OlderDto {
                loaded: 0,
                more: false,
                primary_offline: false,
                message: None,
            });
        };
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            if let Some((loaded, more)) = self.lock().page_answer(&req) {
                return Ok(OlderDto {
                    loaded,
                    more,
                    primary_offline: false,
                    message: None,
                });
            }
            if tokio::time::Instant::now() >= deadline {
                return Ok(OlderDto {
                    loaded: 0,
                    more: true,
                    primary_offline: true,
                    message: Some(t(
                        "Abra o Zoen no celular para carregar mensagens antigas.",
                        "Open Zoen on your phone to load older messages.",
                    )),
                });
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
