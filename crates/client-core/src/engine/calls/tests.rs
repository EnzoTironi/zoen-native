use super::*;

struct Fixture {
    engine: Engine,
    log: SpaceLog,
    caller: Author,
    callee: Author,
    other_device: Author,
}
impl Fixture {
    fn new() -> Self {
        let mut engine = Engine::open(":memory:").unwrap();
        fn person(engine: &mut Engine, name: &str) -> Signer {
            let id = engine
                .create_identity(
                    Identity {
                        id: String::new(),
                        kind: IdentityKind::Person,
                        name: name.into(),
                        handle: name.into(),
                        tint_hex: "#000000".into(),
                        glyph: None,
                        owner: None,
                        bio: String::new(),
                        owner_proof: None,
                    },
                    true,
                )
                .unwrap();
            engine.signers[&id].clone()
        }
        let a = person(&mut engine, "caller");
        let b = person(&mut engine, "callee");
        let caller = Author::device(&a, Signer::generate());
        let callee = Author::device(&b, Signer::generate());
        let other_device = Author::device(&b, Signer::generate());
        engine.store.set_meta("me", &a.id()).unwrap();
        let mut f = Self {
            engine,
            log: SpaceLog::new("sp_calls"),
            caller,
            callee,
            other_device,
        };
        f.push(
            f.caller.clone(),
            0,
            EventBody::SpaceCreated {
                title: "Calls".into(),
                kind: SpaceKind::Direct,
                privacy: Privacy::EndToEnd,
            },
        );
        f.push(
            f.caller.clone(),
            1,
            EventBody::MemberAdded {
                identity: b.id(),
                role: Role::Member,
            },
        );
        f
    }
    fn event(&self, by: &Author, at: i64, signal: CallSignal) -> Event {
        by.sign_event(
            "sp_calls",
            &new_ulid(at),
            at,
            self.log.head(),
            EventBody::CallV1 { signal },
        )
    }
    fn push(&mut self, by: Author, at: i64, body: EventBody) -> Event {
        let event = by.sign_event("sp_calls", &new_ulid(at), at, self.log.head(), body);
        let event = self.log.sequence(event).clone();
        self.engine.store.append_event(&event).unwrap();
        self.engine.reload().unwrap();
        event
    }
    fn invite(&mut self) -> CallRecord {
        self.push(
            self.caller.clone(),
            100,
            EventBody::CallV1 {
                signal: CallSignal::Invite {
                    call_id: format!("call_{}", "a".repeat(32)),
                    callee: self.callee.identity.clone(),
                    media: CallMedia::Audio,
                    expires_at_ms: 1100,
                },
            },
        );
        self.record()
    }
    fn record(&self) -> CallRecord {
        self.engine.state.spaces["sp_calls"]
            .calls
            .last()
            .unwrap()
            .clone()
    }
    fn accept(&self, call: &CallRecord) -> CallSignal {
        CallSignal::Accept {
            call_id: call.call_id.clone(),
            invitation: call.invitation.clone(),
        }
    }
    fn end(&self, call: &CallRecord, reason: CallEndReason) -> CallSignal {
        CallSignal::End {
            call_id: call.call_id.clone(),
            invitation: call.invitation.clone(),
            reason,
        }
    }
    fn valid(&self, event: &Event) -> bool {
        self.engine
            .state
            .call_event_valid(event, &self.engine.identities)
    }
}

#[test]
fn sqlite_cold_replay_keeps_signed_invitation_acceptance_and_terminal_history() {
    let mut f = Fixture::new();
    let call = f.invite();
    assert!(call.invitation_confirmed && call.ringing_at(100));
    let accept = f.accept(&call);
    f.push(f.callee.clone(), 200, EventBody::CallV1 { signal: accept });
    assert_eq!(f.record().accepted_device, f.callee.device);
    let end = f.end(&call, CallEndReason::HungUp);
    f.push(f.caller.clone(), 300, EventBody::CallV1 { signal: end });
    let before = f.record();
    assert!(before.terminal() && before.acceptance_confirmed);
    let bytes = f.engine.store.memory_image().unwrap();
    let reopened =
        Engine::from_store(Store::from_memory_image(&bytes).unwrap(), ":memory:").unwrap();
    assert_eq!(reopened.state.spaces["sp_calls"].calls, vec![before]);
}

#[test]
fn two_devices_accepting_choose_first_canonical_device_and_reject_late_callbacks() {
    let mut f = Fixture::new();
    let call = f.invite();
    let accept = f.accept(&call);
    let second = f.event(&f.other_device, 201, accept.clone());
    f.push(f.callee.clone(), 200, EventBody::CallV1 { signal: accept });
    assert!(!f.valid(&second));
    assert!(!f.valid(&f.event(&f.other_device, 300, f.end(&call, CallEndReason::HungUp))));
    assert!(f.valid(&f.event(&f.callee, 202, f.accept(&call))));
    assert_eq!(f.record().accepted_device, f.callee.device);
}

#[test]
fn cancellation_before_accept_is_terminal_and_accept_before_cancel_requires_hangup() {
    let mut f = Fixture::new();
    let call = f.invite();
    let end = f.end(&call, CallEndReason::Cancelled);
    f.push(f.caller.clone(), 200, EventBody::CallV1 { signal: end });
    assert!(!f.valid(&f.event(&f.callee, 201, f.accept(&call))));
    let mut f = Fixture::new();
    let call = f.invite();
    let accept = f.accept(&call);
    f.push(f.callee.clone(), 200, EventBody::CallV1 { signal: accept });
    assert!(!f.valid(&f.event(&f.caller, 201, f.end(&call, CallEndReason::Cancelled))));
    assert!(f.valid(&f.event(&f.caller, 201, f.end(&call, CallEndReason::HungUp))));
}

#[test]
fn wrong_invitation_author_device_and_expired_acceptance_have_no_projection_effect() {
    let mut f = Fixture::new();
    let call = f.invite();
    let before = f.record();
    assert!(!f.valid(&f.event(&f.caller, 200, f.accept(&call))));
    assert!(!f.valid(&f.event(&f.callee, 99, f.accept(&call))));
    assert!(!f.valid(&f.event(&f.callee, 1100, f.accept(&call))));
    let mut bad = f.event(&f.callee, 200, f.accept(&call));
    bad.device = None;
    assert!(!f.valid(&bad));
    let bad = CallSignal::Accept {
        call_id: call.call_id.clone(),
        invitation: "other authenticated invite".into(),
    };
    assert!(!f.valid(&f.event(&f.callee, 200, bad)));
    assert_eq!(before, f.record());
    assert!(!call.ringing_at(1100));
}

#[test]
fn membership_change_invalidates_existing_call_in_cold_replay() {
    let mut f = Fixture::new();
    let call = f.invite();
    let accept = f.accept(&call);
    f.push(f.callee.clone(), 200, EventBody::CallV1 { signal: accept });
    f.push(
        f.caller.clone(),
        300,
        EventBody::MemberRemoved {
            identity: f.callee.identity.clone(),
        },
    );
    assert!(f.record().invalidated && f.record().terminal());
    assert!(!f.valid(&f.event(&f.callee, 400, f.accept(&call))));
}

#[test]
fn call_signal_encoding_is_closed_and_legacy_messages_unchanged() {
    let invalid = serde_json::json!({"CallV1":{"signal":{"type":"accept","call_id":"call_a","invitation":"hash","role":"admin"}}});
    assert!(serde_json::from_value::<EventBody>(invalid).is_err());
    let f = Fixture::new();
    assert_eq!(
        event_log::content::decode_body(br#"{"CallV2":{"room":"x"}}"#),
        EventBody::Unsupported {
            kind: "CallV2".into()
        }
    );
    assert!(f.engine.state.spaces["sp_calls"].calls.is_empty());
}

#[test]
fn signed_glare_reprojects_to_first_canonical_invitation_and_rejects_duplicate_ids() {
    let mut f = Fixture::new();
    let call = f.invite();
    let second = CallSignal::Invite {
        call_id: format!("call_{}", "b".repeat(32)),
        callee: f.caller.identity.clone(),
        media: CallMedia::Video,
        expires_at_ms: 1200,
    };
    assert!(!f.valid(&f.event(&f.callee, 200, second)));
    let duplicate = CallSignal::Invite {
        call_id: call.call_id,
        callee: f.callee.identity.clone(),
        media: CallMedia::Video,
        expires_at_ms: 1300,
    };
    assert!(!f.valid(&f.event(&f.caller, 300, duplicate)));
    assert_eq!(f.engine.state.spaces["sp_calls"].calls.len(), 1);
}

#[test]
fn no_call_authority_in_group_clear_space_agent_dm_or_root_signed_event() {
    let mut f = Fixture::new();
    let signal = CallSignal::Invite {
        call_id: format!("call_{}", "b".repeat(32)),
        callee: f.callee.identity.clone(),
        media: CallMedia::Video,
        expires_at_ms: 1200,
    };
    let event = f.event(&f.caller, 200, signal);
    assert!(f.valid(&event));
    f.engine.state.spaces.get_mut("sp_calls").unwrap().kind = SpaceKind::Group;
    assert!(!f.valid(&event));
    f.engine.state.spaces.get_mut("sp_calls").unwrap().kind = SpaceKind::Direct;
    f.engine.state.spaces.get_mut("sp_calls").unwrap().privacy = Privacy::Closed;
    assert!(!f.valid(&event));
    f.engine.state.spaces.get_mut("sp_calls").unwrap().privacy = Privacy::EndToEnd;
    f.engine
        .identities
        .get_mut(&f.callee.identity)
        .unwrap()
        .kind = IdentityKind::Agent;
    assert!(!f.valid(&event));
    f.engine
        .identities
        .get_mut(&f.callee.identity)
        .unwrap()
        .kind = IdentityKind::Person;
    let mut root_event = event;
    root_event.device = None;
    assert!(!f.valid(&root_event));
}

#[test]
fn excessive_lifetime_and_stale_account_command_are_refused_before_writes() {
    let mut f = Fixture::new();
    for expiry in [200, 199, 200 + MAX_RING_MS + 1, i64::MAX] {
        let signal = CallSignal::Invite {
            call_id: format!("call_{}", "b".repeat(32)),
            callee: f.callee.identity.clone(),
            media: CallMedia::Audio,
            expires_at_ms: expiry,
        };
        assert!(!f.valid(&f.event(&f.caller, 200, signal)));
    }
    let before = f.engine.store.event_count().unwrap();
    assert!(f
        .engine
        .invite_call(
            &f.caller.identity,
            f.caller.device.as_deref().unwrap(),
            "sp_calls",
            false
        )
        .is_err());
    assert_eq!(f.engine.store.event_count().unwrap(), before);
    assert!(f.engine.store.outbox().unwrap().is_empty());
}

#[test]
fn terminal_call_never_rings_on_cold_replay_or_duplicate_end() {
    let mut f = Fixture::new();
    let call = f.invite();
    let end = f.end(&call, CallEndReason::Declined);
    f.push(
        f.callee.clone(),
        200,
        EventBody::CallV1 {
            signal: end.clone(),
        },
    );
    let before = f.record();
    assert!(!before.ringing_at(201));
    f.push(f.callee.clone(), 250, EventBody::CallV1 { signal: end });
    assert_eq!(before, f.record());
    assert_eq!(f.engine.state.spaces["sp_calls"].calls.len(), 1);
}
