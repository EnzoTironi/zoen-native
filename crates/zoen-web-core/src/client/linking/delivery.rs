//! Implementation within the existing browser client authority.
use super::*;

impl<S: Signer> Client<S> {
    pub(in crate::client) fn linking_reply(
        &mut self,
        id: u64,
        result: &Result<Reply, String>,
    ) -> Result<bool, String> {
        if let Some(link) = self
            .saved
            .linking
            .handover
            .as_mut()
            .filter(|link| link.request_id == id)
        {
            match result {
                Ok(Reply::Done) => {
                    link.delivered = true;
                    link.sealed.clear();
                    self.saved.linking.certified.insert(link.device.clone());
                }
                Err(error) => link.failure = Some(error.clone()),
                _ => link.failure = Some("O servidor não confirmou o vínculo.".into()),
            }
            return Ok(true);
        }
        if let Some(sending) = self
            .saved
            .linking
            .sending
            .as_mut()
            .filter(|sending| sending.request_id == id)
        {
            match result {
                Ok(Reply::Done) => {
                    sending.delivered = true;
                    sending.chunks.clear();
                    sending.sealed_manifest.clear();
                }
                Err(error) => sending.failure = Some(error.clone()),
                _ => sending.failure = Some("O servidor não confirmou o histórico.".into()),
            }
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn prepare_outgoing_history(&mut self, device: &str) -> Result<(), String> {
        if self.saved.membership.revoked.contains(device) {
            return Err("Este aparelho foi removido da conta.".into());
        }
        if let Some(sending) = &mut self.saved.linking.sending {
            if sending.device == device {
                sending.failure = None;
                self.linking_net.history_due = 0;
                return Ok(());
            }
            if !sending.delivered {
                return Err("Termine o envio de histórico em andamento.".into());
            }
        }
        if !self.saved.linking.certified.contains(device)
            || self.saved.membership.revoked.contains(device)
        {
            return Err("Este aparelho não foi vinculado aqui.".into());
        }
        let peer = self
            .saved
            .linking
            .peers
            .get(device)
            .ok_or("A chave do vínculo não está disponível.")?;
        let mut events = Vec::new();
        for (space, log) in &self.logs {
            if !self.roster(space).contains_key(&self.account.identity) {
                continue;
            }
            let opened = log
                .events()
                .iter()
                .filter(|event| event.sealed_wire.is_some())
                .collect::<Vec<_>>();
            events.extend(
                opened[opened.len().saturating_sub(500)..]
                    .iter()
                    .map(|event| (*event).clone()),
            );
        }
        if events.len() > 100_000 {
            return Err("Histórico grande demais para este navegador.".into());
        }
        let messages = events.len() as u32;
        let plain = zeroize::Zeroizing::new(
            serde_json::to_vec(&device_link::Bundle { events })
                .map_err(|_| "Histórico inválido.")?,
        );
        if plain.len() > 64 << 20 {
            return Err("Histórico grande demais para este navegador.".into());
        }
        let key = zeroize::Zeroizing::new(device_link::random::<32>());
        let transfer = hex::encode(device_link::random::<32>());
        let chunks = plain
            .chunks(1 << 20)
            .enumerate()
            .map(|(index, part)| device_link::seal_chunk(&key, index as u32, part))
            .collect::<Vec<_>>();
        let manifest = device_link::HistoryBox {
            transfer: transfer.clone(),
            key: hex::encode(key.as_ref()),
            chunks: chunks
                .iter()
                .map(|chunk| event_log::sha256_hex(chunk))
                .collect(),
        };
        let manifest = zeroize::Zeroizing::new(
            serde_json::to_vec(&manifest).map_err(|_| "Histórico inválido.")?,
        );
        let secret = zeroize::Zeroizing::new(peer.secret().ok_or("Chave de vínculo inválida.")?);
        let sealed_manifest = device_link::seal(
            &hex::decode(&peer.their_hpke).map_err(|_| "Aparelho inválido.")?,
            &secret,
            "history",
            &manifest,
        )
        .ok_or("Não foi possível proteger o histórico.")?;
        let request_id = self.next_retained_request_id()?;
        self.saved.linking.sending = Some(Sending {
            request_id,
            device: device.into(),
            transfer,
            chunks,
            uploaded: 0,
            sealed_manifest,
            delivered: false,
            messages,
            hint_sent: false,
            failure: None,
        });
        Ok(())
    }

    pub(super) fn outgoing_history_work(&self, now: i64) -> Result<Value, String> {
        let Some(sending) = &self.saved.linking.sending else {
            return Ok(json!({"kind":"idle"}));
        };
        if self.saved.membership.revoked.contains(&sending.device) {
            return Err("Este aparelho foi removido da conta.".into());
        }
        if let Some(error) = &sending.failure {
            return Err(error.clone());
        }
        if sending.delivered {
            return Ok(json!({"kind":"complete","messages":sending.messages}));
        }
        if let Some(chunk) = sending.chunks.get(sending.uploaded) {
            let index = sending.uploaded;
            let sha = event_log::sha256_hex(chunk);
            let payload =
                protocol::transfer_message("put", &sending.transfer, &index.to_string(), &sha, now);
            let signature = self
                .signer
                .sign(&payload)
                .map_err(|_| "Não foi possível autorizar o histórico.")?;
            return Ok(
                json!({"kind":"upload","transfer":sending.transfer,"index":index,"sha256":sha,"bytes":chunk,"device":self.account.device,"timestamp":now.to_string(),"signature":hex::encode(signature)}),
            );
        }
        Ok(json!({"kind":"waiting"}))
    }

    pub(in crate::client) fn linking_frames(
        &mut self,
        frames: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        if !self.synced {
            return Ok(());
        }
        if let Some(link) = &self.saved.linking.handover {
            if !link.delivered && link.failure.is_none() && self.linking_net.due <= self.view_now_ms
            {
                frames.push(
                    ClientFrame::Req {
                        id: link.request_id,
                        op: Op::DeliverLink {
                            id: link.box_id.clone(),
                            sealed: link.sealed.clone(),
                            device: Some(protocol::DeviceCertificate {
                                device: link.device.clone(),
                                cert: link.cert.clone(),
                            }),
                        },
                    }
                    .encode(),
                );
                self.linking_net.due = self.view_now_ms.saturating_add(5000);
            }
        }
        if let Some(sending) = self.saved.linking.sending.as_ref().filter(|sending| {
            !sending.delivered
                && sending.failure.is_none()
                && sending.uploaded == sending.chunks.len()
                && !self.saved.membership.revoked.contains(&sending.device)
                && self.linking_net.history_due <= self.view_now_ms
        }) {
            let secret = self
                .saved
                .linking
                .peers
                .get(&sending.device)
                .and_then(Peer::secret)
                .ok_or("Chave de vínculo indisponível.")?;
            frames.push(
                ClientFrame::Req {
                    id: sending.request_id,
                    op: Op::DeliverLink {
                        id: device_link::box_id(&secret, "history"),
                        sealed: sending.sealed_manifest.clone(),
                        device: None,
                    },
                }
                .encode(),
            );
            self.linking_net.history_due = self.view_now_ms.saturating_add(5000);
        }
        if let Some(sending) = self.saved.linking.sending.as_mut().filter(|sending| {
            sending.delivered
                && !sending.hint_sent
                && !self.saved.membership.revoked.contains(&sending.device)
        }) {
            if let Some(sealed) = self
                .saved
                .linking
                .peers
                .get(&sending.device)
                .and_then(|peer| peer.seal(&DeviceMsg::HistoryReady))
            {
                frames.push(
                    ClientFrame::Req {
                        id: DEVICE_REQUEST,
                        op: Op::SendDevice {
                            to: sending.device.clone(),
                            sealed,
                        },
                    }
                    .encode(),
                );
                sending.hint_sent = true;
            }
        }
        let mut joins = Vec::new();
        for device in self
            .saved
            .linking
            .certified
            .iter()
            .filter(|device| !self.saved.membership.revoked.contains(*device))
        {
            for space in self.logs.keys().filter(|space| {
                self.metadata(space)
                    .is_some_and(|(_, _, p)| p == Privacy::EndToEnd)
                    && self.roster(space).contains_key(&self.account.identity)
                    && self
                        .saved
                        .linking
                        .joins
                        .get(device)
                        .is_none_or(|joined| !joined.contains(*space))
            }) {
                joins.push((space.clone(), device.clone()));
            }
        }
        for (space, device) in joins {
            self.enqueue_control(
                &space,
                EventBody::DeviceJoining {
                    device: device.clone(),
                },
                self.view_now_ms,
            )?;
            self.saved
                .linking
                .joins
                .entry(device)
                .or_default()
                .insert(space);
        }
        for (space, pending) in &self.saved.linking.pages {
            if self
                .saved
                .membership
                .revoked
                .contains(&self.account.primary)
                || pending.answer.is_some()
                || pending.failure.is_some()
                || self
                    .linking_net
                    .page_due
                    .get(space)
                    .is_some_and(|due| *due > self.view_now_ms)
            {
                continue;
            }
            let peer = Peer {
                device: self.account.primary.clone(),
                their_hpke: self.account.primary_hpke.clone(),
                my_hpke_secret: self.account.hpke_secret.clone(),
                secret: self.account.link_secret.clone(),
            };
            if let Some(sealed) = peer.seal(&DeviceMsg::PageRequest {
                req: pending.req.clone(),
                space: space.clone(),
                before: pending.before,
                limit: PAGE_LIMIT as u32,
            }) {
                frames.push(
                    ClientFrame::Req {
                        id: DEVICE_REQUEST,
                        op: Op::SendDevice {
                            to: peer.device,
                            sealed,
                        },
                    }
                    .encode(),
                );
                self.linking_net
                    .page_due
                    .insert(space.clone(), self.view_now_ms.saturating_add(5000));
            }
        }
        Ok(())
    }
}
