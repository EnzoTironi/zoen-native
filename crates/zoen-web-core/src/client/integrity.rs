//! Read-only browser log verification and bounded audit projection.
use super::*;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    IntegrityWarnings {},
    VerifyLog {
        space_id: String,
    },
    LogEventsPage {
        space_id: String,
        before: Option<String>,
        limit: u32,
    },
}

impl<S: Signer> Client<S> {
    pub fn integrity_request(&self, request: &str) -> Result<Value, String> {
        if request.len() > 16_384 {
            return Err("Audit request too large.".into());
        }
        let command: Command =
            serde_json::from_str(request).map_err(|_| "Invalid audit request.")?;
        if matches!(command, Command::IntegrityWarnings {}) {
            return Ok(json!(self
                .saved
                .integrity_errors
                .iter()
                .filter(|(space, _)| self.roster(space).contains_key(&self.account.identity))
                .collect::<BTreeMap<_, _>>()));
        }
        let space = match &command {
            Command::IntegrityWarnings {} => unreachable!(),
            Command::VerifyLog { space_id } | Command::LogEventsPage { space_id, .. } => space_id,
        };
        if !self.roster(space).contains_key(&self.account.identity) {
            return Err("Conversation unavailable to this account.".into());
        }
        let log = self.logs.get(space).ok_or("Conversation unavailable.")?;
        match command {
            Command::IntegrityWarnings {} => unreachable!(),
            Command::VerifyLog { space_id } => {
                let result = SpaceLog::from_events(&space_id, log.events().to_vec())
                    .map_err(|e| e.to_string());
                let error = self
                    .saved
                    .integrity_errors
                    .get(&space_id)
                    .cloned()
                    .or_else(|| result.err())
                    // A verified hash chain may still contain conflicting Item
                    // identities. Reuse the same accepted fold on explicit audit.
                    .or_else(|| self.document_items().err());
                Ok(
                    json!({"space_id":space_id,"space_title":self.metadata(&space_id).map(|m|m.0).unwrap_or_default(),
                    "events":log.len().to_string(),"head_hash":if error.is_none(){log.head_hash()}else{""},"valid":error.is_none(),"error":error}),
                )
            }
            Command::LogEventsPage {
                space_id,
                before,
                limit,
            } => {
                if !(1..=100).contains(&limit) {
                    return Err("Audit page size must be between 1 and 100.".into());
                }
                let before = before
                    .map(|cursor| -> Result<u64, String> {
                        let (scope, seq, hash): (String, u64, String) =
                            serde_json::from_str(&cursor).map_err(|_| "Invalid audit cursor.")?;
                        if scope != space_id
                            || !log.events().iter().any(|e| e.seq == seq && e.hash == hash)
                        {
                            return Err("The audit cursor changed. Reload this report.".into());
                        }
                        Ok(seq)
                    })
                    .transpose()?;
                let mut entries = log
                    .events()
                    .iter()
                    .rev()
                    .filter(|e| before.is_none_or(|seq| e.seq < seq));
                let events: Vec<_> = entries.by_ref().take(limit as usize).collect();
                let next = if entries.next().is_some() {
                    events.last().map(|e| {
                        serde_json::to_string(&(&space_id, e.seq, &e.hash)).expect("audit cursor")
                    })
                } else {
                    None
                };
                let events: Vec<_> = events.into_iter().map(|e| {
                    let label = match &e.body {
                        EventBody::SpaceCreated { .. } => "Conversa criada",
                        EventBody::MemberAdded { .. } => "Participante adicionado",
                        EventBody::MemberRemoved { .. } => "Participante removido",
                        EventBody::MessagePosted { .. } => "Mensagem",
                        EventBody::MessageAnnotated { .. } => "Atualização de mensagem",
                        EventBody::SpaceEncrypted => "Criptografia ativada",
                        EventBody::Checkpoint { .. } => "Conferência de membros",
                        _ => "Evento do histórico",
                    };
                    json!({"seq":e.seq.to_string(),"label":label,"author":self.persona_value(&e.author),"at_ms":e.at_ms.to_string(),"hash":e.hash,"prev":e.prev,"signature":e.sig})
                }).collect();
                Ok(json!({"events":events,"before":next}))
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::super::tests::{account, clear, HostSigner};
    use super::*;
    use event_log::{Author, Signer as NativeSigner};

    fn fixture() -> std::result::Result<(Client<HostSigner>, String), Box<dyn std::error::Error>> {
        let identity = NativeSigner::from_secret(&[201; 32]);
        let device = NativeSigner::from_secret(&[202; 32]);
        let author = Author::device(&identity, device.clone());
        let mut client = Client::new(account(&identity, &device), HostSigner(device), "").unwrap();
        let space = "sp_integrity_fixture".to_string();
        let mut log = SpaceLog::new(&space);
        client
            .receive(
                &clear(
                    &mut log,
                    &author,
                    EventBody::SpaceCreated {
                        title: "Audit fixture".into(),
                        kind: SpaceKind::Group,
                        privacy: Privacy::Closed,
                    },
                    1,
                )?,
                1,
            )
            .unwrap();
        for now in 2..7 {
            client
                .receive(
                    &clear(
                        &mut log,
                        &author,
                        EventBody::MessagePosted {
                            text: format!("private {now}"),
                            attaches: None,
                            message: format!("msg_{now}"),
                            reply: None,
                            media: None,
                            forwarded: false,
                        },
                        now,
                    )?,
                    now,
                )
                .unwrap();
        }
        Ok((client, space))
    }

    #[test]
    fn integrity_verification_and_bounded_cursor_do_not_mutate_or_expose_bodies(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let (client, space) = fixture()?;
        let request = |op: &str, before: Value| {
            json!({"op":op,"space_id":space,"before":before,"limit":2}).to_string()
        };
        let report = client
            .integrity_request(&json!({"op":"verify_log","space_id":space}).to_string())
            .unwrap();
        assert_eq!(report["valid"], true);
        let first = client
            .integrity_request(&request("log_events_page", Value::Null))
            .unwrap();
        assert_eq!(first["events"].as_array().unwrap().len(), 2);
        assert!(!first.to_string().contains("private"));
        let second = client
            .integrity_request(&request("log_events_page", first["before"].clone()))
            .unwrap();
        assert!(
            first["events"][1]["seq"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
                > second["events"][0]["seq"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
        );
        assert!(client
            .integrity_request(
                &json!({"op":"log_events_page","space_id":space,"before":null,"limit":101})
                    .to_string()
            )
            .is_err());
        assert!(client
            .integrity_request(&json!({"op":"verify_log","space_id":"sp_unknown"}).to_string())
            .is_err());
        let wrong_scope = serde_json::to_string(&("sp_other", 2u64, "hash")).unwrap();
        assert!(client
            .integrity_request(&request("log_events_page", json!(wrong_scope)))
            .is_err());
        let mut changed = client;
        let outsider = NativeSigner::from_secret(&[203; 32]);
        let mut other_log = SpaceLog::new("sp_not_a_member");
        let outsider_author = Author::root(outsider);
        changed
            .receive(
                &clear(
                    &mut other_log,
                    &outsider_author,
                    EventBody::SpaceCreated {
                        title: "Other account".into(),
                        kind: SpaceKind::Group,
                        privacy: Privacy::Closed,
                    },
                    9,
                )?,
                9,
            )
            .unwrap();
        assert!(changed
            .integrity_request(&json!({"op":"verify_log","space_id":"sp_not_a_member"}).to_string())
            .is_err());
        assert!(changed.integrity_request(&json!({"op":"log_events_page","space_id":"sp_not_a_member","before":null,"limit":1}).to_string()).is_err());
        changed
            .saved
            .integrity_errors
            .insert(space.clone(), "Conflicting encrypted membership".into());
        assert_eq!(
            changed
                .integrity_request(&json!({"op":"verify_log","space_id":space}).to_string())
                .unwrap()["valid"],
            false
        );
        assert_eq!(
            changed
                .integrity_request(r#"{"op":"integrity_warnings"}"#)
                .unwrap()[&space],
            "Conflicting encrypted membership"
        );
        changed
            .saved
            .integrity_errors
            .insert("sp_not_a_member".into(), "not mine".into());
        assert!(changed
            .integrity_request(r#"{"op":"integrity_warnings"}"#)
            .unwrap()
            .get("sp_not_a_member")
            .is_none());
        changed.logs.remove(&space);
        assert!(changed
            .integrity_request(&json!({"op":"verify_log","space_id":space}).to_string())
            .is_err());
        Ok(())
    }

    #[test]
    fn corrupted_snapshot_cannot_be_reopened_as_verified_history(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let (mut client, space) = fixture()?;
        let snapshot = client.update(Vec::new()).unwrap().snapshot;
        let mut saved: Value = serde_json::from_str(&snapshot).unwrap();
        saved["logs"][&space][1]["hash"] = json!("corrupted");
        assert!(Client::new(
            client.account.clone(),
            client.signer.clone(),
            &saved.to_string()
        )
        .is_err());
        Ok(())
    }
}
