//! Complete reply authority scenarios; original registrations stay in the parent.

use super::*;

pub(super) fn prepared_reply_preserves_exact_native_outbox_through_cold_restore_and_confirmation(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("publication-owner-trust", Some(1000))?;
    let trigger = fixture.message("Original reply request")?;
    let facts = fixture.core.reply(SPACE, &trigger, 400).unwrap();
    let original = facts.intent().clone();
    let frontier = facts.frontier().clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Retained model result\n", 500)
        .unwrap();
    assert_eq!(prepared.facts().intent(), &original);
    assert_eq!(prepared.facts().frontier(), &frontier);
    assert_eq!(prepared.facts().expires_at_ms(), Some(1000));
    let envelope = prepared.envelope().clone();
    envelope.verify().unwrap();
    assert!(envelope.is_sealed() && !envelope.is_pruned());
    assert_eq!(envelope.author(), fixture.agent_profile.id);
    assert_eq!(envelope.device(), Some(fixture.agent_device.id().as_str()));
    assert_eq!(envelope.cert.as_ref(), Some(&fixture.agent_cert));
    let wire = ClientFrame::Publish {
        env: envelope.clone(),
    }
    .encode();
    let mut cold = DeviceCore::restore(
        NativeImage::decode_bounded(prepared.image().as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    let pending = cold.engine.store.outbox().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].event.client_id, envelope.client_id());
    assert_eq!(
        event_log::content_hash_of(&pending[0].event),
        prepared.event_digest()
    );
    assert!(matches!(
        &pending[0].event.body,
        EventBody::MessagePosted { message, text, attaches: None, reply: None, media: None, forwarded: false, }
            if message == prepared.message() && text == "Retained model result\n"
    ));
    assert_eq!(cold.engine.outbox_envelopes(), vec![envelope.clone()]);
    assert_eq!(
        ClientFrame::Publish {
            env: cold.engine.outbox_envelopes().remove(0)
        }
        .encode(),
        wire
    );
    let frame = ordered(&fixture.owner, envelope)?;
    assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Applied);
    let received = fixture.owner.logs[SPACE].events().last().unwrap();
    assert!(matches!(
        &received.body,
        EventBody::MessagePosted { message, text, attaches: None, reply: None, media: None, forwarded: false, }
            if message == prepared.message() && text == "Retained model result\n"
    ));
    cold.ingest(vec![frame.clone()]).unwrap();
    assert_eq!(cold.engine.store.outbox_len().unwrap(), 0);
    assert_eq!(cold.retained_head(SPACE).unwrap().unwrap().hash, frame.hash);
    assert!(cold.engine.logs[SPACE]
        .events()
        .iter()
        .all(|event| !matches!(event.body, EventBody::UsageRecorded { .. })));
    println!("JOURNEY prepared reply keeps certified sealed wire and own pending effect in complete cold image; actual accepted echo clears outbox without native usage charge");

    Ok(())
}

pub(super) fn preparation_refuses_expired_replaced_revoked_and_reader_authority(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for case in ["expired", "replaced", "revoked", "reader"] {
        let mut fixture = Fixture::new()?;
        fixture.grant("publication-original-trust", Some(1000))?;
        let trigger = fixture.message("Original retained request")?;
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        match case {
            "expired" => {}
            "replaced" => {
                fixture.grant("publication-replacement-trust", None)?;
            }
            "revoked" => {
                fixture.encrypted(
                    EventBody::GrantRevoked {
                        grant: "publication-original-trust".into(),
                    },
                    450,
                )?;
            }
            "reader" => {
                fixture.clear(EventBody::MemberAdded {
                    identity: fixture.agent_profile.id.clone(),
                    role: Role::Reader,
                })?;
            }
            _ => unreachable!(),
        }
        let before = fixture.core.image().unwrap();
        let at_ms = if case == "expired" { 1000 } else { 500 };
        assert!(matches!(
            fixture
                .core
                .prepare_reply(&original, "Retained output", at_ms),
            Err(NativeError::Permission)
        ));
        assert_eq!(fixture.core.engine.store.outbox_len().unwrap(), 0);
        assert!(matches!(fixture.core.image(), Err(NativeError::History)));
        let restored = DeviceCore::restore(before, fixture.credential()).unwrap();
        assert_eq!(restored.engine.store.outbox_len().unwrap(), 0);
    }
    println!("JOURNEY native reply preparation refuses actual expiry, replacement, revocation and reader role; failed scratch images cannot activate");

    Ok(())
}

pub(super) fn concurrent_receipt_prefix_restores_only_the_matching_certified_device(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("concurrent-prefix-trust", None)?;
    let trigger = fixture.message("Concurrent receipt input")?;
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let prepared = fixture
        .core
        .prepare_reply(&original, "Retained concurrent output", 500)
        .unwrap();
    let cid = prepared.envelope().client_id().to_string();
    let pending = fixture.core.snapshot_outbox().unwrap();
    let next = receipt_source_message(&mut fixture, 0)?;
    assert_eq!(next.env.seen(), prepared.envelope().seen());
    assert_ne!(next.env.author(), prepared.envelope().author());
    next.env.verify().unwrap();
    let ReplyReceipt::Prefix(prefix) = fixture
        .core
        .receive_reply_progress(&original, &cid, vec![next])
        .unwrap()
    else {
        panic!("the concurrent owner message is not the reply receipt");
    };
    assert!(fixture.core.snapshot_outbox().unwrap() == pending);
    let other = Fixture::new_in("other-certified-device")?;
    assert!(matches!(
        DeviceCore::restore(
            NativeImage::decode_bounded(prefix.image().as_bytes().to_vec()).unwrap(),
            other.credential(),
        ),
        Err(NativeError::Credential)
    ));
    let mut cold = DeviceCore::restore(
        NativeImage::decode_bounded(prefix.image().as_bytes().to_vec()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    assert!(cold.snapshot_outbox().unwrap() == pending);
    let accepted = ordered(&fixture.owner, prepared.envelope().clone())?;
    assert_eq!(fixture.owner.ingest(accepted.clone()), Ingest::Applied);
    let ReplyReceipt::Accepted(receipt) = cold
        .receive_reply_progress(&original, &cid, vec![accepted.clone()])
        .unwrap()
    else {
        panic!("the original pending reply must produce acceptance");
    };
    assert_eq!(receipt.accepted().hash, accepted.hash);
    assert_eq!(receipt.reference_digest(), prepared.reference_digest());
    assert_eq!(receipt.publish_digest(), prepared.publish_digest());
    assert_eq!(cold.engine.store.outbox_len().unwrap(), 0);
    println!("JOURNEY concurrent same-prefix owner message preserves the original pending reply through cold restore; a different real certified agent is refused and only the original reply produces acceptance");

    Ok(())
}

pub(super) fn preparation_bounds_text_and_never_reseals_an_existing_pending_reply(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("publication-bounded-trust", None)?;
    let trigger = fixture.message("Original bounded request")?;
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let before = fixture.core.image().unwrap().as_bytes().to_vec();
    for text in [
        String::new(),
        " \n\t".into(),
        "x".repeat(MAX_REPLY_BYTES + 1),
    ] {
        let mut scratch = DeviceCore::restore(
            NativeImage::decode_bounded(before.clone()).unwrap(),
            fixture.credential(),
        )
        .unwrap();
        assert!(matches!(
            scratch.prepare_reply(&original, &text, 500),
            Err(NativeError::Bounds)
        ));
        assert_eq!(scratch.engine.store.outbox_len().unwrap(), 0);
        assert!(matches!(scratch.image(), Err(NativeError::History)));
    }
    let text = "é".repeat(MAX_REPLY_BYTES / 2);
    let prepared = fixture.core.prepare_reply(&original, &text, 500).unwrap();
    let wire = ClientFrame::Publish {
        env: prepared.envelope().clone(),
    }
    .encode();
    assert!(wire.len() <= MAX_PUBLISH_BYTES);
    assert!(prepared.envelope().content().len() <= MAX_EVENT_BYTES);
    let image = prepared.image().as_bytes().to_vec();
    let mut cold = DeviceCore::restore(
        NativeImage::decode_bounded(image.clone()).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    assert!(matches!(
        cold.prepare_reply(&original, "Second output", 501),
        Err(NativeError::NotReady)
    ));
    assert!(matches!(cold.image(), Err(NativeError::History)));
    let restored = DeviceCore::restore(
        NativeImage::decode_bounded(image).unwrap(),
        fixture.credential(),
    )
    .unwrap();
    assert_eq!(restored.engine.store.outbox_len().unwrap(), 1);
    assert_eq!(
        ClientFrame::Publish {
            env: restored.engine.outbox_envelopes().remove(0)
        }
        .encode(),
        wire
    );
    println!("JOURNEY real 32 KiB UTF-8 reply fits native wire/image bounds; empty, oversized and repeated pending preparation refuse without resealing");

    Ok(())
}

pub(super) fn failed_reply_sealing_discards_scratch_and_preserves_committed_image(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    fixture.grant("publication-sealing-trust", None)?;
    let trigger = fixture.message("Original sealing request")?;
    let original = fixture
        .core
        .reply(SPACE, &trigger, 400)
        .unwrap()
        .intent()
        .clone();
    let before = fixture.core.image().unwrap();
    fixture.core.engine.store.conn().execute_batch(
        "CREATE TRIGGER fail_reply_seal BEFORE INSERT ON meta WHEN NEW.key LIKE 'mls.sealed:%' BEGIN SELECT RAISE(ABORT,'injected sealed outbox failure'); END;"
    ).unwrap();
    assert!(matches!(
        fixture
            .core
            .prepare_reply(&original, "Output requiring durable sealing", 500),
        Err(NativeError::Storage)
    ));
    assert!(matches!(fixture.core.image(), Err(NativeError::History)));
    let mut restored = DeviceCore::restore(before, fixture.credential()).unwrap();
    assert_eq!(restored.engine.store.outbox_len().unwrap(), 0);
    let prepared = restored
        .prepare_reply(&original, "Output requiring durable sealing", 500)
        .unwrap();
    prepared.envelope().verify().unwrap();
    println!("JOURNEY real sealed outbox write failure poisons scratch after native append; prior committed image remains usable with no loose native effect");

    Ok(())
}

pub(super) fn no_default_or_optimistic_grant_can_start_a_reply(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut fixture = Fixture::new()?;
    let trigger = fixture.message("No explicit grant yet")?;
    assert!(matches!(
        fixture.core.reply(SPACE, &trigger, 400),
        Err(NativeError::Permission)
    ));
    let pending = fixture.owner.net.author.clone().unwrap().sign_event(
        SPACE,
        "pending-trust",
        200,
        fixture.owner.logs.get(SPACE).and_then(SpaceLog::head),
        EventBody::GrantIssued {
            grant: Grant {
                id: "optimistic-owner-trust".into(),
                grantor: fixture.owner_profile.id.clone(),
                grantee: Some(fixture.agent_profile.id.clone()),
                scope: GrantScope::Space(SPACE.into()),
                capability: Capability::Trust(TrustLevel::Autonomous),
                expires_at_ms: None,
            },
        },
    )?;
    fixture.core.engine.store.outbox_put(&pending).unwrap();
    fixture.core.engine.reload().unwrap();
    assert!(fixture
        .core
        .engine
        .state
        .grants
        .iter()
        .any(|g| g.grant.id == "optimistic-owner-trust"));
    assert!(matches!(
        fixture.core.reply(SPACE, &trigger, 400),
        Err(NativeError::Permission)
    ));
    fixture.grant("real-owner-trust", None)?;
    let real = fixture.message("Explicit opened owner trust")?;
    assert!(fixture.core.reply(SPACE, &real, 400).is_ok());
    println!("JOURNEY native optimistic overlay and default trust excluded from ordered reply permission");

    Ok(())
}

pub(super) fn revoke_replacement_reader_and_opaque_history_stop_the_original_reply(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for case in ["revoke", "replace", "reader", "opaque", "unsupported"] {
        let mut fixture = Fixture::new()?;
        fixture.grant("initial-trust", None)?;
        let trigger = fixture.message("Initial retained request")?;
        let original = fixture
            .core
            .reply(SPACE, &trigger, 400)
            .unwrap()
            .intent()
            .clone();
        match case {
            "revoke" => {
                fixture.encrypted(
                    EventBody::GrantRevoked {
                        grant: "initial-trust".into(),
                    },
                    450,
                )?;
            }
            "replace" => {
                fixture.grant("replacement-trust", None)?;
            }
            "reader" => {
                fixture.clear(EventBody::MemberAdded {
                    identity: fixture.agent_profile.id.clone(),
                    role: Role::Reader,
                })?;
            }
            "unsupported" => {
                fixture.encrypted(
                    EventBody::BackgroundSet {
                        background: BackgroundSpec {
                            style: "none".into(),
                            media: None,
                            zoom_pm: 1000,
                            offset_x_pm: 0,
                            offset_y_pm: 0,
                            dim_pm: None,
                            blur_pm: 0,
                            appearance: "auto".into(),
                        },
                    },
                    450,
                )?;
            }
            "opaque" => {
                let author = fixture.owner.net.author.as_ref().unwrap();
                let envelope = Envelope::sealed(
                    author,
                    SPACE,
                    "unreadable",
                    450,
                    fixture.owner.logs[SPACE].head().as_ref(),
                    Sealed::new(SealedKind::Application, ::mls::SUITE_ID, vec![1, 2, 3]),
                );
                let frame = ordered(&fixture.owner, envelope)?;
                fixture.core.ingest(vec![frame]).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(
                fixture.core.refresh(&original, 500),
                Err(NativeError::Permission)
            ),
            "{case}"
        );
    }
    println!("JOURNEY exact grant revoke/replacement, Reader role, opaque and unsupported history deny refresh");

    Ok(())
}

pub(super) fn certified_device_and_owner_proof_are_required_without_root_fallback(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let mut agent = fixture.agent_profile.clone();
    agent.owner_proof = None;
    assert!(matches!(
        UnlockedDevice::verify(
            agent,
            fixture.owner_profile.clone(),
            fixture.agent_cert.clone(),
            fixture.agent_device.secret()
        ),
        Err(NativeError::Credential)
    ));
    assert!(matches!(
        UnlockedDevice::verify(
            fixture.agent_profile.clone(),
            fixture.owner_profile.clone(),
            "00".repeat(64),
            fixture.agent_device.secret()
        ),
        Err(NativeError::Credential)
    ));
    assert!(matches!(
        UnlockedDevice::verify(
            fixture.agent_profile.clone(),
            fixture.owner_profile.clone(),
            fixture.agent_cert.clone(),
            Signer::generate()?.secret()
        ),
        Err(NativeError::Credential)
    ));
    let mut owner = fixture.owner_profile.clone();
    owner.kind = IdentityKind::Agent;
    assert!(matches!(
        UnlockedDevice::verify(
            fixture.agent_profile.clone(),
            owner,
            fixture.agent_cert.clone(),
            fixture.agent_device.secret()
        ),
        Err(NativeError::Credential)
    ));
    let agent_root = Signer::generate()?;
    let owner_author = fixture.owner.net.author.as_ref().unwrap();
    let mut agent = profile(&agent_root, IdentityKind::Agent);
    agent.owner = Some(fixture.owner_profile.id.clone());
    agent.owner_proof = Some(Box::new(
        agent_owner::authorize(owner_author, &agent.id).unwrap(),
    ));
    let self_certificate = agent_root.sign(&event_log::device_cert_message(&agent_root.id()));
    assert!(matches!(
        UnlockedDevice::verify(
            agent,
            fixture.owner_profile.clone(),
            self_certificate,
            agent_root.secret()
        ),
        Err(NativeError::Credential)
    ));
    println!("JOURNEY unsigned owner, false certificate, wrong device, non-Person owner and Agent root fallback refused");

    Ok(())
}
