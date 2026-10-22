use super::*;

impl Session {
    /// Tells each claimed device that's running low, wherever it is connected.
    pub(super) async fn key_packages_low(&self, claimed: &[protocol::KeyPackageRecord]) {
        let devices: Vec<(String, String)> = claimed
            .iter()
            .map(|k| (k.identity.clone(), k.device.clone()))
            .collect();
        let stock = match db::key_package_stock(&self.st.pool, &devices).await {
            Ok(stock) => stock,
            Err(error) => {
                tracing::warn!(%error, "claimed key-package stock query failed");
                return;
            }
        };
        for (identity, device, left) in stock {
            if left < protocol::KEY_PACKAGES_LOW as i64 {
                let delivered_here = self.st.fanout.send(
                    std::slice::from_ref(&identity),
                    &ServerFrame::KeyPackagesLow {
                        device: device.clone(),
                        remaining: left as u32,
                    },
                    None,
                );
                tracing::debug!(identity = %pseudo(&identity), device = %pseudo(&device), remaining = left, delivered_here, "low key-package notice");
            }
        }
    }

    pub(super) async fn go_online(&mut self) {
        if self.hub_id.is_some() {
            return;
        }
        self.st.analytics.session(&self.identity);
        self.hub_id = Some(self.st.fanout.add(
            &self.identity,
            Mailbox {
                tx: self.tx.clone(),
                kick: self.kick.clone(),
                device: self.device.clone(),
                revoked: self.revoked.clone(),
            },
        ));
        // Attach before reading stock: a claim racing the snapshot then reaches this
        // mailbox, or is reflected in the query. Reading first can lose both signals.
        // Packages claimed while this device was away: it refills as it comes back.
        self.send_key_package_stock().await;
        let st = self.st.clone();
        let identity = self.identity.clone();
        let tx = self.tx.clone();
        // The mutex makes Session Sync for its &self async methods; only this
        // session's frame loop ever accesses the future, through get_mut (no lock).
        self.presence = Some(std::sync::Mutex::new(Box::pin(async move {
            if let Ok(Ok(co)) =
                tokio::time::timeout(Duration::from_secs(2), st.log.co_members(&identity)).await
            {
                // Always announce this attach. A redundant online announcement is
                // harmless. Probing remote presence here would put NATS back on
                // the login critical path; connecting-device refill races sync on
                // hub attachment instead of a pre-attach self-ping.
                st.fanout.send(
                    &co,
                    &ServerFrame::Presence {
                        identity: identity.clone(),
                        online: true,
                    },
                    None,
                );
                // Every device receives a snapshot. Mailbox backpressure pauses only
                // this future, not login, Ping, Sync or authorization refreshes.
                for who in st.fanout.online(&co).await {
                    if tx
                        .send(ServerFrame::Presence {
                            identity: who,
                            online: true,
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        })));
    }

    pub(super) async fn send_key_package_stock(&self) {
        let me = [(self.identity.clone(), self.device.clone())];
        if let Ok(stock) = db::key_package_stock(&self.st.pool, &me).await {
            for (_, device, left) in stock {
                if left < protocol::KEY_PACKAGES_LOW as i64 {
                    self.send(ServerFrame::KeyPackagesLow {
                        device,
                        remaining: left as u32,
                    })
                    .await;
                }
            }
        }
    }

    /// Device bucket first, then the account's: one device can't spend its siblings' share.
    pub(super) fn publish_allowed(&self) -> Result<(), std::time::Duration> {
        let l = &self.st.limits;
        l.publish_device.check(&self.device)?;
        l.publish_account.check(&self.identity)
    }

    pub(super) async fn still_authorized(&self) -> bool {
        if self.revoked.load(Ordering::Acquire) {
            return false;
        }
        match db::authorize_device(
            &self.st.session_auth,
            &self.identity,
            &self.device,
            self.require_device.load(Ordering::Acquire),
        )
        .await
        {
            Ok(Some(tx)) => {
                drop(tx);
                true
            }
            _ => {
                self.revoked.store(true, Ordering::Release);
                self.kick.notify_one();
                false
            }
        }
    }

    /// Wakes the devices of every member with no live session. The push carries nothing;
    /// the app syncs, decrypts and decides itself whether to notify. Under overload,
    /// best-effort wakes are dropped before allocation/spawn, not queued.
    pub(super) fn wake_away(&self, audience: &[String]) {
        if !self.st.push.enabled() {
            return;
        }
        let Some(permit) = self.st.push.try_wake_task() else {
            return;
        };
        let st = self.st.clone();
        let me = self.identity.clone();
        let audience: Vec<String> = audience.iter().filter(|a| **a != me).cloned().collect();
        tokio::spawn(async move {
            let _permit = permit;
            let online: std::collections::HashSet<String> =
                st.fanout.online(&audience).await.into_iter().collect();
            let away: Vec<String> = audience
                .into_iter()
                .filter(|a| !online.contains(a))
                .collect();
            if away.is_empty() {
                return;
            }
            let Ok(targets) = db::push_targets(&st.pool, &away).await else {
                return;
            };
            for t in targets {
                match st.push.wake(&t).await {
                    crate::push::Outcome::Gone => {
                        let _ = db::drop_push_token(&st.pool, &t.token).await;
                    }
                    crate::push::Outcome::Failed(error) => {
                        tracing::warn!(platform = %t.platform, %error, "wake push failed")
                    }
                    _ => {}
                }
            }
        });
    }
}
