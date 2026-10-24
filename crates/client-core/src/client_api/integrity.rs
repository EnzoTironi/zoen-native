//! Read-only integrity boundary. Verification always rereads canonical storage.
use super::{encode, invalid, Client, CoreError, LogEventsPage};

impl Client {
    fn integrity_scope(engine: &crate::engine::Engine, space: &str) -> Result<(), CoreError> {
        let me = engine.me_id()?;
        if !engine.is_member(space, &me) {
            return Err(CoreError::Forbidden {
                reason: "Conversation unavailable to this account.".into(),
            });
        }
        Ok(())
    }

    pub(super) fn client_integrity_warnings(&self) -> std::collections::BTreeMap<String, String> {
        let engine = self.lock();
        let Ok(me) = engine.me_id() else {
            return Default::default();
        };
        engine
            .state
            .spaces
            .iter()
            .filter_map(|(id, state)| {
                if !state.members.iter().any(|(member, _)| member == &me) {
                    return None;
                }
                state
                    .integrity_error
                    .as_ref()
                    .map(|error| (id.clone(), error.clone()))
            })
            .collect()
    }

    pub(super) fn client_verify_log(&self, space: String) -> Result<String, CoreError> {
        let engine = self.lock();
        Self::integrity_scope(&engine, &space)?;
        let mut report = engine.verify_log(&space);
        if let Some(error) = engine
            .state
            .spaces
            .get(&space)
            .and_then(|state| state.integrity_error.clone())
            .or_else(|| engine.recorded_fork(&space))
        {
            report.valid = false;
            report.head_hash.clear();
            report.error = Some(error);
        }
        encode(&report)
    }

    pub(super) fn client_log_events_page(
        &self,
        space: String,
        before: Option<String>,
        limit: u32,
    ) -> Result<String, CoreError> {
        if !(1..=100).contains(&limit) {
            return Err(invalid("Audit page size must be between 1 and 100."));
        }
        let engine = self.lock();
        Self::integrity_scope(&engine, &space)?;
        let log = engine
            .logs
            .get(&space)
            .ok_or_else(|| invalid("Audit unavailable for this conversation."))?;
        let before = before
            .map(|cursor| -> Result<u64, CoreError> {
                let (scope, seq, hash): (String, u64, String) =
                    serde_json::from_str(&cursor).map_err(|_| invalid("Invalid audit cursor."))?;
                if scope != space || !log.events().iter().any(|e| e.seq == seq && e.hash == hash) {
                    return Err(invalid("The audit cursor changed. Reload this report."));
                }
                Ok(seq)
            })
            .transpose()?;
        let mut entries = log
            .events()
            .iter()
            .rev()
            .filter(|e| before.is_none_or(|seq| e.seq < seq));
        let events: Vec<_> = entries
            .by_ref()
            .take(limit as usize)
            .map(|e| crate::LogEventDto {
                seq: e.seq,
                label: crate::engine::event_label(&e.body),
                author: engine.persona(&e.author),
                at_ms: e.at_ms,
                hash: e.hash.clone(),
                prev: e.prev.clone(),
                signature: e.sig.clone(),
            })
            .collect();
        let next = if entries.next().is_some() {
            events
                .last()
                .map(|e| serde_json::to_string(&(&space, e.seq, &e.hash)).expect("audit cursor"))
        } else {
            None
        };
        encode(&LogEventsPage {
            events,
            before: next,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[tokio::test]
    async fn cold_open_warns_about_unreadable_history_without_trusting_its_membership() {
        let directory = std::env::temp_dir().join(types::new_id("integrity-cold-open").unwrap());
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory
            .join("account.sqlite")
            .to_string_lossy()
            .into_owned();
        let space = {
            let core = Client::open(path.clone(), "en".into()).unwrap();
            core.seed_demo_if_empty().unwrap();
            let snapshot = core.client_snapshot();
            assert!(!snapshot.has_unreadable_local_history);
            let space = snapshot.conversations[0].id.clone();
            let engine = core.lock();
            let mut event = engine.store.events(&space).unwrap().remove(0);
            // The JSON remains readable, but its signed log is no longer valid.
            event.hash = "0".repeat(64);
            engine
                .store
                .conn()
                .execute(
                    "UPDATE events SET json = ?1 WHERE space = ?2 AND seq = ?3",
                    rusqlite::params![serde_json::to_string(&event).unwrap(), space, event.seq],
                )
                .unwrap();
            space
        };
        let core = Client::open(path, "en".into()).unwrap();
        let snapshot = core.client_snapshot();
        assert!(snapshot.has_unreadable_local_history);
        assert!(!snapshot.integrity_warnings.contains_key(&space));
        assert!(core.lock().broken_spaces[&space].members.is_empty());
        assert!(matches!(
            core.client_verify_log(space.clone()),
            Err(CoreError::Forbidden { .. })
        ));
        assert!(matches!(
            core.client_log_events_page(space, None, 10),
            Err(CoreError::Forbidden { .. })
        ));
        drop(core);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn integrity_reads_disk_and_pages_authorized_metadata_only() {
        let core = Client::open(":memory:".into(), "en".into()).unwrap();
        core.seed_demo_if_empty().unwrap();
        let snapshot: Value = serde_json::from_str(
            &core
                .client_request(json!({"op":"snapshot"}).to_string())
                .await
                .unwrap(),
        )
        .unwrap();
        let space = snapshot["conversations"][0]["id"].as_str().unwrap();
        assert!(core.client_integrity_warnings().is_empty());
        core.lock()
            .state
            .spaces
            .get_mut(space)
            .unwrap()
            .integrity_error = Some("known fork".into());
        assert_eq!(
            core.client_integrity_warnings()
                .get(space)
                .map(String::as_str),
            Some("known fork")
        );
        let known: Value =
            serde_json::from_str(&core.client_verify_log(space.into()).unwrap()).unwrap();
        assert_eq!(known["valid"], false);
        core.lock()
            .state
            .spaces
            .get_mut(space)
            .unwrap()
            .integrity_error = None;
        let report = core.client_verify_log(space.into()).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&report).unwrap()["valid"],
            true
        );
        let first: Value =
            serde_json::from_str(&core.client_log_events_page(space.into(), None, 1).unwrap())
                .unwrap();
        assert_eq!(first["events"].as_array().unwrap().len(), 1);
        assert!(first["events"][0].get("body").is_none());
        if let Some(before) = first["before"].as_str() {
            let next: Value = serde_json::from_str(
                &core
                    .client_log_events_page(space.into(), Some(before.into()), 1)
                    .unwrap(),
            )
            .unwrap();
            assert_ne!(first["events"][0]["hash"], next["events"][0]["hash"]);
            let invalid = serde_json::to_string(&("sp_other", 0, "hash")).unwrap();
            assert!(core
                .client_log_events_page(space.into(), Some(invalid), 1)
                .is_err());
        }
        assert!(core
            .client_log_events_page(space.into(), None, 101)
            .is_err());
        assert!(core.client_verify_log("sp_missing".into()).is_err());
        core.lock()
            .store
            .raw_execute(&format!(
                "UPDATE events SET json = '{{}}' WHERE space = '{}'",
                space
            ))
            .unwrap();
        let corrupt: Value =
            serde_json::from_str(&core.client_verify_log(space.into()).unwrap()).unwrap();
        assert_eq!(corrupt["valid"], false);
        assert_eq!(corrupt["head_hash"], "");
        {
            let mut engine = core.lock();
            let me = engine.me_id().unwrap();
            engine
                .state
                .spaces
                .get_mut(space)
                .unwrap()
                .members
                .retain(|(id, _)| id != &me);
        }
        assert!(core.client_integrity_warnings().is_empty());
        assert!(core.client_verify_log(space.into()).is_err());
        assert!(core.client_log_events_page(space.into(), None, 1).is_err());
    }
}
