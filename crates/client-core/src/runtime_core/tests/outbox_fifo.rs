//! Authenticated native custody keeps the exact old queue across schema upgrade.
use super::*;

fn previous_outbox(store: &Store) {
    // Recreate the exact released table SQL, preserving the legacy insertion
    // positions and every retained byte/retry field, as an old capsule would.
    store.conn().execute_batch(&format!(
        "CREATE TEMP TABLE old_pending AS SELECT queue_order,client_id,space,json,created_ms,attempts,last_error,failed FROM outbox;
         DROP TABLE outbox;
         {};
         INSERT INTO outbox(rowid,client_id,space,json,created_ms,attempts,last_error,failed)
         SELECT queue_order,client_id,space,json,created_ms,attempts,last_error,failed FROM old_pending ORDER BY queue_order;
         DROP TABLE old_pending;
         CREATE INDEX outbox_by_space ON outbox (space, created_ms);",
        super::super::image_schema::PRE_FIFO_OUTBOX,
    )).unwrap();
}

pub(super) fn native_image_upgrades_only_exact_previous_outbox_schema_without_changing_signed_queue(
) {
    let mut fixture = Fixture::new();
    let author = fixture.core.engine.net.author.clone().unwrap();
    let queued = [("first", 300), ("second", 100), ("third", 100)]
        .into_iter()
        .map(|(text, at)| {
            fixture
                .core
                .engine
                .append_synced(
                    SPACE,
                    &author,
                    at,
                    EventBody::MessagePosted {
                        message: new_id("fifo"),
                        text: text.into(),
                        attaches: None,
                        reply: None,
                        media: None,
                        forwarded: false,
                    },
                )
                .unwrap()
        })
        .collect::<Vec<_>>();
    fixture.core.engine.mls_seal_outbox().unwrap();
    fixture
        .core
        .engine
        .store
        .outbox_note(&queued[0].client_id, "retained retry", false)
        .unwrap();
    let original = fixture.core.engine.outbox_envelopes();
    let current = fixture.core.image().unwrap();
    for alteration in [
        None,
        Some("ALTER TABLE outbox ADD COLUMN surprise TEXT"),
        Some("CREATE INDEX injected_outbox_index ON outbox(space)"),
    ] {
        let store = Store::from_memory_image(current.as_bytes()).unwrap();
        previous_outbox(&store);
        if let Some(sql) = alteration {
            store.conn().execute_batch(sql).unwrap();
        }
        let old_schema = schema(&store).unwrap();
        let image = NativeImage::decode_bounded(store.memory_image().unwrap()).unwrap();
        let restored = DeviceCore::restore(image, fixture.credential());
        if alteration.is_some() {
            assert!(
                matches!(restored, Err(NativeError::Image)),
                "{alteration:?}"
            );
        } else {
            let cold = restored.unwrap();
            assert_eq!(cold.engine.outbox_envelopes(), original);
            let pending = cold.engine.store.outbox().unwrap();
            assert_eq!(
                pending.iter().map(|p| &p.event).collect::<Vec<_>>(),
                queued.iter().collect::<Vec<_>>()
            );
            assert_eq!(pending[0].attempts, 1);
            assert_eq!(pending[0].last_error.as_deref(), Some("retained retry"));
            assert_eq!(
                schema(&cold.engine.store).unwrap(),
                schema(&fixture.core.engine.store).unwrap()
            );
            let again = DeviceCore::restore(cold.image().unwrap(), fixture.credential()).unwrap();
            assert_eq!(again.engine.outbox_envelopes(), original);
        }
        assert_eq!(
            schema(&store).unwrap(),
            old_schema,
            "input capsule stays immutable"
        );
    }
}
