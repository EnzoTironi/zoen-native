#[derive(Default)]
struct Vault(Mutex<HashMap<String, Vec<u8>>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}

struct Device {
    core: Arc<Client>,
    account: AccountDto,
    path: PathBuf,
}
impl Device {
    fn new(w: &World, name: &str, handle: &str, relay: String) -> Self {
        let path = w.dir.join(format!("{handle}.sqlite"));
        let core = Client::open(path.to_string_lossy().into_owned(), "en-US".into()).unwrap();
        let account = core
            .create_account(
                name.into(),
                handle.into(),
                relay,
                Arc::new(Vault::default()),
            )
            .unwrap();
        Self {
            core,
            account,
            path,
        }
    }
    async fn start(&self) {
        self.core.start_sync(None).unwrap();
        let status = self.core.wait_until_idle(15_000).await;
        assert_eq!(status.state, "online", "{status:?}");
        assert!(status.synced, "{status:?}");
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        self.core.stop_sync();
    }
}

fn target_count(device: &Device, space: &str, id: &str) -> usize {
    device
        .core
        .timeline(space.into())
        .unwrap_or_default()
        .iter()
        .filter(|e| e.id == id)
        .count()
}

async fn wait_entry(device: &Device, space: &str, id: &str, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if target_count(device, space, id) == 1 {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "local projection missing {id}; {:?}",
            device.core.connection()
        );
        // timeline only reads the local engine; it does not send Sync or open the chat.
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Group creation starts real profile-key exchange as well as content delivery.
/// Settle those durable writes and the seed receipt BEFORE choosing the final tail.
/// No quiet-window sleep, fake receipt, explicit Sync or production switch is used.
async fn settled_setup_prefix(
    w: &World,
    ana: &Device,
    bruno: &Device,
    proxy: &FinalEventProxy,
    space: &str,
    seed_id: &str,
    evidence: &std::path::Path,
) -> Vec<protocol::Sequenced> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let canonical = w.events_in(space).await;
        let directed_share = |from: &str, to: &str| {
            canonical.iter().any(|event| {
                event.env.author() == from
                    && event.plain_event().is_some_and(|plain| {
                        matches!(plain.body,
                    types::EventBody::ProfileKeyShared { version, shares }
                        if version > 0 && shares.iter().any(|share| share.to == to))
                    })
            })
        };
        let shares_complete = directed_share(&ana.account.identity_id, &bruno.account.identity_id)
            && directed_share(&bruno.account.identity_id, &ana.account.identity_id);
        let sender_seed = ana
            .core
            .timeline(space.into())
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == seed_id);
        let seed_received = sender_seed
            .as_ref()
            .is_some_and(|entry| matches!(entry.delivery, Delivery::Delivered | Delivery::Read));
        let records = proxy.snapshot();
        let receipt_sent = sender_seed.as_ref().is_some_and(|entry| {
            records.iter().any(|record| {
                matches!(&record.kind, Kind::Delivered { space: receipt_space, seq }
                if receipt_space == space && *seq >= entry.seq)
            })
        });
        let canonical_seen_on_receiver_wire = canonical.iter().all(|event| records.iter().any(|record| {
            matches!(&record.kind, Kind::Event { space: event_space, id, seq, hash, dropped: false }
                if event_space == space && id == event.env.client_id() && *seq == event.seq && hash == &event.hash)
        }));
        // SpaceSummary.event_count is the accepted SpaceLog length / next sequence,
        // not the visible message count or optimistic outbox length (engine.rs).
        let ana_next = ana.core.space(space.into()).unwrap().event_count;
        let bruno_next = bruno.core.space(space.into()).unwrap().event_count;
        let canonical_next = canonical.last().map_or(0, |event| event.seq + 1);
        if shares_complete
            && seed_received
            && receipt_sent
            && canonical_seen_on_receiver_wire
            && ana_next == canonical_next
            && bruno_next == canonical_next
        {
            // These public methods only observe the actual clients' full idle contract,
            // including pending profile work and outbox. They do not request a refresh.
            let ana_settled = ana.core.wait_until_settled(500).await.is_ok();
            let bruno_settled = bruno.core.wait_until_settled(500).await.is_ok();
            if ana_settled && bruno_settled && w.events_in(space).await == canonical {
                let plain = canonical
                    .iter()
                    .map(|event| event.plain_event().unwrap())
                    .collect::<Vec<_>>();
                event_log::SpaceLog::from_events(space.to_owned(), plain.clone()).unwrap();
                std::fs::write(
                    evidence.join("settled-setup-fdb-prefix.json"),
                    serde_json::to_vec_pretty(&plain).unwrap(),
                )
                .unwrap();
                std::fs::write(
                    evidence.join("settled-setup-witness.json"),
                    serde_json::to_vec_pretty(&serde_json::json!({
                        "seed_id": seed_id,
                        "seed_seq": sender_seed.as_ref().unwrap().seq,
                        "seed_delivery": format!("{:?}", sender_seed.as_ref().unwrap().delivery),
                        "real_receiver_delivered_frame_observed": true,
                        "both_canonical_profile_key_share_directions_complete": true,
                        "canonical_next_seq": canonical_next,
                        "sender_accepted_next_seq": ana_next,
                        "receiver_accepted_next_seq": bruno_next,
                        "all_prefix_events_seen_on_receiver_wire": true,
                        "both_actual_clients_settled": true
                    }))
                    .unwrap(),
                )
                .unwrap();
                return canonical;
            }
        }
        assert!(tokio::time::Instant::now() < deadline,
            "initial profile shares, exact accepted cursors and seed delivery must settle before fault; shares_complete={shares_complete}, seed_received={seed_received}, receipt_sent={receipt_sent}, canonical_next={canonical_next}, ana_next={ana_next}, bruno_next={bruno_next}; canonical={canonical:#?}; wire={records:#?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn assert_same_healthy_socket(records: &[Record], connection: u64) {
    assert_eq!(
        records
            .iter()
            .filter(|r| matches!(r.kind, Kind::Open))
            .count(),
        1,
        "receiver reconnected: {records:#?}"
    );
    assert!(
        records.iter().all(|r| r.connection == connection),
        "receiver changed socket"
    );
    for record in records {
        match &record.kind {
            Kind::Close => panic!(
                "receiver transport closed at {}ms: {records:#?}",
                record.elapsed_ms
            ),
            Kind::TransportError(message) | Kind::RelayError(message) => panic!(
                "receiver transport failed at {}ms: {message}; {records:#?}",
                record.elapsed_ms
            ),
            _ => {}
        }
    }
}
