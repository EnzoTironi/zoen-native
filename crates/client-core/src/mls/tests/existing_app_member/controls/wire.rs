//! Signed, genuinely MLS-sealed adversarial payloads exercise the consumer, not
//! only the reviewed producer. No fabricated MLS state or imported past keys.
use super::*;
use types::AppCurrentShare;
mod continuity;

pub(super) fn template(f: &mut LateWord) -> R<AppCurrentShare> {
    receipt(f, "template")?;
    f.owner
        .store
        .outbox()?
        .into_iter()
        .find_map(|row| match row.event.body {
            EventBody::AppCurrentShared { share } => Some(*share),
            _ => None,
        })
        .ok_or_else(|| storage("actual pending canonical share"))
}

fn sealed(f: &LateWord, share: AppCurrentShare, id: &str) -> R<Sequenced> {
    sealed_intent(f, share, id).map(|(frame, _)| frame)
}
fn sealed_intent(f: &LateWord, mut share: AppCurrentShare, id: &str) -> R<(Sequenced, Event)> {
    let author = f
        .owner
        .net
        .author
        .as_ref()
        .ok_or_else(|| storage("owner signer"))?;
    share.mutation = id.into();
    share.grant.id = types::new_id("gr");
    let e = author.sign_event(
        SPACE,
        id,
        crate::engine::now_ms(),
        Some(share.cut.clone()),
        EventBody::AppCurrentShared {
            share: Box::new(share),
        },
    );
    let data = f
        .owner
        .device()?
        .seal(
            SPACE,
            &event_log::content::InnerEvent {
                content: e.content.clone(),
                sig: e.sig.clone(),
            }
            .encode(),
        )
        .map_err(|e| storage(&e.to_string()))?;
    let frame = ordered(
        &f.agent,
        Envelope::sealed(
            author,
            SPACE,
            &e.client_id,
            e.at_ms,
            e.seen.as_ref(),
            event_log::content::Sealed::new(SealedKind::Application, SUITE_ID, data),
        ),
    );
    Ok((frame, e))
}

#[test]
fn malformed_or_overprivileged_snapshot_rejects_item_and_grant_together() -> R<()> {
    for case in [
        "schema",
        "version",
        "target",
        "scope",
        "trust",
        "issuer",
        "registry",
        "anchor",
        "cut",
        "epoch",
        "origin_absent",
        "origin_space",
        "origin_device",
        "origin_author",
        "origin_mutation",
    ] {
        let mut f = late_word(":memory:", ":memory:")?;
        let mut share = template(&mut f)?;
        match case {
            "schema" => share.schema = 0,
            "version" => share.version = 0,
            "target" => share.target = "not_a_member".into(),
            "scope" => share.grant.scope = types::GrantScope::Space(SPACE.into()),
            "trust" => {
                share.grant.capability = types::Capability::Trust(types::TrustLevel::Autonomous)
            }
            "issuer" => share.grant.grantor = "foreign".into(),
            "registry" => share.content.resource_uri = "ui://foreign.html".into(),
            "anchor" => share.anchor.hash = "00".repeat(32),
            "cut" => share.cut.hash = "00".repeat(32),
            "epoch" => share.epoch = share.epoch.map(|epoch| epoch + 1),
            "origin_absent" => share.content.document.as_mut().unwrap().app_origin = None,
            field => {
                let proof = share
                    .content
                    .document
                    .as_mut()
                    .unwrap()
                    .app_origin
                    .as_mut()
                    .unwrap();
                match field {
                    "origin_space" => proof.space = "foreign".into(),
                    "origin_device" => proof.device = "foreign".into(),
                    "origin_author" => proof.author = "foreign".into(),
                    _ => proof.mutation = "foreign".into(),
                }
            }
        }
        let before = counts(&f.agent);
        let frame = sealed(&f, share, case)?;
        if case == "cut" {
            assert!(
                matches!(f.agent.ingest(frame), Ingest::Invalid(_)),
                "bad causal address must be refused before retention"
            );
            assert_eq!(counts(&f.agent), before);
            continue;
        }
        assert_eq!(
            f.agent.ingest(frame),
            Ingest::Applied,
            "wire must authenticate/decrypt: {case}"
        );
        assert_eq!(
            f.agent.state.grants.len(),
            before.3,
            "no orphan capability: {case}"
        );
        assert_eq!(f.agent.state.items.len(), before.2, "no snapshot: {case}");
        assert!(f.agent.item_state(&f.original.item_id).is_err(), "{case}");
        assert!(
            matches!(
                f.agent.logs[SPACE].events().last().map(|e| &e.body),
                Some(EventBody::AppCurrentShared { .. })
            ),
            "not a decryption false positive: {case}"
        );
    }
    Ok(())
}

#[test]
fn retained_readable_successor_refuses_stale_floor_and_its_grant_atomically() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    let mut stale = template(&mut f)?;
    // Keep the already signed intent pending locally while delivering only a
    // separate genuine post-join ItemVersioned to the receiver first.
    let author = f
        .owner
        .net
        .author
        .as_ref()
        .ok_or_else(|| storage("owner signer"))?;
    let mut app = stale.content.clone();
    app.title = "Retained successor".into();
    let event = author.sign_event(
        SPACE,
        "retained_version",
        crate::engine::now_ms(),
        seen(&f.agent, SPACE),
        EventBody::ItemVersioned {
            item: stale.item.clone(),
            content: ItemContent::App(app),
            note: "real retained event".into(),
        },
    );
    let data = f
        .owner
        .device()?
        .seal(
            SPACE,
            &event_log::content::InnerEvent {
                content: event.content.clone(),
                sig: event.sig.clone(),
            }
            .encode(),
        )
        .map_err(|e| storage(&e.to_string()))?;
    let frame = ordered(
        &f.agent,
        Envelope::sealed(
            author,
            SPACE,
            &event.client_id,
            event.at_ms,
            event.seen.as_ref(),
            event_log::content::Sealed::new(SealedKind::Application, SUITE_ID, data),
        ),
    );
    assert_eq!(f.agent.ingest(frame), Ingest::Applied);
    assert!(matches!(
        f.agent.logs[SPACE].events().last().map(|e| &e.body),
        Some(EventBody::ItemVersioned { .. })
    ));
    assert!(f.agent.item_state(&stale.item).is_err());
    stale.cut = seen(&f.agent, SPACE).ok_or_else(|| storage("new canonical cut"))?;
    let before = counts(&f.agent);
    let bad = sealed(&f, stale.clone(), "stale_floor_two")?;
    assert_eq!(f.agent.ingest(bad), Ingest::Applied);
    assert_eq!(f.agent.state.grants.len(), before.3);
    assert!(f.agent.item_state(&stale.item).is_err());
    Ok(())
}

#[test]
fn canonical_share_then_save_with_reordered_delivery_converges_without_lost_successor() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    receipt(&mut f, "ordered_share")?;
    // Use the real sealed publication, confirming it only at the owner so that
    // the peer receives the next version before the missing canonical share.
    f.owner.mls_seal_outbox()?;
    let envelopes = f.owner.outbox_envelopes();
    assert_eq!(envelopes.len(), 1);
    let first = ordered(&f.owner, envelopes[0].clone());
    assert_eq!(f.owner.ingest(first.clone()), Ingest::Confirmed);
    let base = f.original.clone();
    let v3 = save(&mut f, &base, wordcraft::EMPTY, "ordered_three")?;
    f.blobs.extend(f.owner.pending_uploads(100));
    for (sha, _) in f.owner.pending_uploads(100) {
        f.owner.upload_done(&sha);
    }
    f.owner.mls_seal_outbox()?;
    let envelopes = f.owner.outbox_envelopes();
    assert_eq!(envelopes.len(), 1);
    let next = ordered(&f.owner, envelopes[0].clone());
    assert_eq!(f.owner.ingest(next.clone()), Ingest::Confirmed);
    assert_eq!(
        f.agent.ingest(next.clone()),
        Ingest::Gap { next: first.seq }
    );
    assert!(f.agent.item_state(&base.item_id).is_err());
    assert_eq!(f.agent.ingest(first.clone()), Ingest::Applied);
    assert_eq!(f.agent.ingest(next.clone()), Ingest::Applied);
    receive_wanted(&mut f.agent, &f.blobs)?;
    assert_eq!(
        f.agent
            .app_binary_read(&base.item_id, Some(3), Some(&v3.sha256))?
            .data_base64,
        v3.data_base64
    );
    assert_eq!(
        f.agent
            .item(&base.item_id)?
            .versions
            .iter()
            .map(|v| v.number)
            .collect::<Vec<_>>(),
        vec![3, 2]
    );
    let before = counts(&f.agent);
    assert_eq!(f.agent.ingest(first), Ingest::Duplicate);
    assert_eq!(f.agent.ingest(next), Ingest::Duplicate);
    assert_eq!(counts(&f.agent), before);
    Ok(())
}

#[test]
fn a_new_delivery_of_old_snapshot_after_revocation_cannot_revive_the_known_app() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    let mut share = template(&mut f)?;
    publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    receive_wanted(&mut f.agent, &f.blobs)?;
    revoke(&mut f)?;
    publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    share.cut = seen(&f.agent, SPACE).ok_or_else(|| storage("revoked cut"))?;
    let before = counts(&f.agent);
    let frame = sealed(&f, share, "not_a_revocation_bypass")?;
    assert_eq!(f.agent.ingest(frame), Ingest::Applied);
    assert_eq!(f.agent.state.grants.len(), before.3);
    assert!(!f.agent.app_trust_available(&f.original.item_id));
    assert_eq!(f.agent.item(&f.original.item_id)?.version, 2);
    Ok(())
}

#[test]
fn a_real_known_item_from_another_conversation_cannot_be_rebound_or_granted() -> R<()> {
    let mut f = late_word(":memory:", ":memory:")?;
    let other = f.owner.create_synced_space(
        "Other actual conversation",
        SpaceKind::Group,
        Privacy::Closed,
        &[f.agent.me_id()?],
    )?;
    let foreign = f.owner.app_binary_create_format(
        Format::Wordcraft,
        &other,
        "Foreign Word",
        &base64::engine::general_purpose::STANDARD.encode(wordcraft::EMPTY),
        "foreign_word",
    )?;
    publish(&mut f.owner, &mut f.agent, &mut f.blobs);
    receive_wanted(&mut f.agent, &f.blobs)?;
    assert_eq!(f.agent.item(&foreign.item_id)?.space_id, other);
    let mut share = template(&mut f)?;
    share.item = foreign.item_id.clone();
    share.grant.scope = types::GrantScope::Item(foreign.item_id.clone());
    share.grant.grantee = Some(format!("app:{}", foreign.item_id));
    let before = counts(&f.agent);
    let frame = sealed(&f, share, "cross_space_item")?;
    assert_eq!(f.agent.ingest(frame), Ingest::Applied);
    assert_eq!(f.agent.state.grants.len(), before.3);
    assert_eq!(f.agent.item(&foreign.item_id)?.space_id, other);
    assert_eq!(f.agent.item(&foreign.item_id)?.version, 1);
    assert!(f.agent.item_state(&f.original.item_id).is_err());
    Ok(())
}
