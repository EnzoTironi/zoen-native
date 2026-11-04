//! Exact original wire on retry, fresh permission, no native mutations.
use super::*;

#[test]
fn pending_reply_reuses_original_wire_and_leaves_native_state_unchanged(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut f = Fixture::new()?;
    f.grant("pending-wire", Some(1000))?;
    let trigger = f.message("Original request")?;
    let original = f.core.reply(SPACE, &trigger, 400)?.intent().clone();
    let prepared = f
        .core
        .prepare_reply(&original, "Exact original reply", 500)?;
    let mut cold = DeviceCore::restore(
        NativeImage::decode_bounded(prepared.image().as_bytes().to_vec())?,
        f.credential(),
    )?;
    let image = cold.image()?;
    let before = cold.snapshot_outbox()?;
    for at in [501, 999] {
        let pending = cold.pending_reply(&original, prepared.envelope().client_id(), at)?;
        assert_eq!(pending.envelope(), prepared.envelope());
        assert_eq!(pending.message(), prepared.message());
        assert_eq!(pending.event_digest(), prepared.event_digest());
        assert_eq!(pending.reference_digest(), prepared.reference_digest());
        assert_eq!(pending.publish_digest(), prepared.publish_digest());
        assert_eq!(pending.facts().intent(), &original);
        assert_eq!(pending.facts().frontier(), prepared.facts().frontier());
        assert!(cold.snapshot_outbox()? == before);
        assert_eq!(cold.image()?.as_bytes(), image.as_bytes());
    }
    assert!(matches!(
        cold.pending_reply(&original, prepared.envelope().client_id(), 1000),
        Err(NativeError::Permission)
    ));
    // A rejected read does not invalidate an otherwise usable scratch image.
    cold.pending_reply(&original, prepared.envelope().client_id(), 600)?;
    let frame = ordered(&f.owner, prepared.envelope().clone())?;
    cold.receive_reply(&original, prepared.envelope().client_id(), vec![frame])?;
    assert!(matches!(
        cold.pending_reply(&original, prepared.envelope().client_id(), 600),
        Err(NativeError::NotReady)
    ));
    Ok(())
}

#[test]
fn pending_reply_refuses_changed_authority_or_original_without_mutating_queue(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    for case in [
        "revoked",
        "replaced",
        "reader",
        "reference",
        "sealing",
        "failed",
        "missing",
        "other-intent",
        "other-client",
    ] {
        let mut f = Fixture::new()?;
        f.grant("pending-refusal", Some(1000))?;
        let trigger = f.message("Original request")?;
        let mut original = f.core.reply(SPACE, &trigger, 400)?.intent().clone();
        let prepared = f.core.prepare_reply(&original, "Original reply", 500)?;
        let client = prepared.envelope().client_id();
        match case {
            "revoked" => {
                f.encrypted(
                    EventBody::GrantRevoked {
                        grant: "pending-refusal".into(),
                    },
                    550,
                )?;
            }
            "replaced" => {
                f.grant("replacement", None)?;
            }
            "reader" => {
                f.clear(EventBody::MemberAdded {
                    identity: f.agent_profile.id.clone(),
                    role: Role::Reader,
                })?;
            }
            "reference" => {
                f.core
                    .engine
                    .store
                    .set_meta(&reference::reference_key(client), "{}")?;
            }
            "sealing" => {
                f.core
                    .engine
                    .store
                    .set_meta(&format!("mls.sealed:{client}"), "invalid")?;
            }
            "failed" => {
                f.core
                    .engine
                    .store
                    .outbox_note(client, "permanently refused", true)?;
            }
            "missing" => {
                f.core.engine.store.outbox_remove(client)?;
            }
            "other-intent" => {
                original.trigger = "b".repeat(64);
            }
            "other-client" => {}
            _ => return Err("unexpected test case".into()),
        }
        let before = f.core.image()?;
        let requested = if case == "other-client" {
            "different-client"
        } else {
            client
        };
        assert!(
            f.core.pending_reply(&original, requested, 600).is_err(),
            "{case}"
        );
        assert_eq!(f.core.image()?.as_bytes(), before.as_bytes(), "{case}");
    }
    Ok(())
}
