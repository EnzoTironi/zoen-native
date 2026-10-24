use super::publication::{MAX_PUBLISH_BYTES, MAX_REPLY_BYTES};
use super::*;
use event_log::{
    chain_hash,
    content::{InnerEvent, Sealed, SealedKind},
    SpaceLog,
};
use protocol::{ClientFrame, Envelope, KeyPackageClaimClock, KeyPackageRecord};

const SPACE: &str = "certified-owner-agent-direct";

mod canonical_approvals;
mod image_delivery_cases;
mod image_review_cases;
mod image_task_cases;
mod owner_policy_cases;

struct Fixture {
    space: String,
    owner: Engine,
    core: DeviceCore,
    owner_profile: Identity,
    agent_profile: Identity,
    agent_device: Signer,
    agent_cert: String,
}

impl Fixture {
    fn credential(&self) -> UnlockedDevice {
        UnlockedDevice::verify(
            self.agent_profile.clone(),
            self.owner_profile.clone(),
            self.agent_cert.clone(),
            self.agent_device.secret(),
        )
        .unwrap()
    }

    fn new() -> std::result::Result<Self, Box<dyn std::error::Error>> {
        Ok(Self::new_in(SPACE)?)
    }

    fn new_in(space_id: &str) -> std::result::Result<Self, Box<dyn std::error::Error>> {
        let owner_root = Signer::generate()?;
        let owner_author = Author::device(&owner_root, Signer::generate()?);
        let owner_profile = profile(&owner_root, IdentityKind::Person);
        let agent_root = Signer::generate()?;
        let agent_device = Signer::generate()?;
        let agent_author = Author::device(&agent_root, agent_device.clone());
        let mut agent_profile = profile(&agent_root, IdentityKind::Agent);
        agent_profile.owner = Some(owner_profile.id.clone());
        agent_profile.owner_proof = Some(Box::new(
            agent_owner::authorize(&owner_author, &agent_root.id()).unwrap(),
        ));
        let agent_cert = agent_author.cert.unwrap();
        let credential = UnlockedDevice::verify(
            agent_profile.clone(),
            owner_profile.clone(),
            agent_cert.clone(),
            agent_device.secret(),
        )
        .unwrap();
        let mut core = DeviceCore::bootstrap(credential, "http://relay.test").unwrap();
        let mut owner = Engine::open(":memory:").unwrap();
        owner.store.put_identity(&owner_profile, None).unwrap();
        owner.store.put_identity(&agent_profile, None).unwrap();
        owner.store.set_meta("me", &owner_profile.id).unwrap();
        owner
            .save_linked_account(AccountMeta {
                identity: owner_profile.id.clone(),
                device: owner_author.key.id(),
                cert: owner_author.cert.clone().unwrap(),
                relay_url: "http://relay.test".into(),
                registered: false,
            })
            .unwrap();
        owner.reload().unwrap();
        owner.net.author = Some(owner_author);
        owner
            .init_profile(&owner_profile.id, &owner_profile.name)
            .unwrap();
        core.engine
            .init_profile(&agent_profile.id, &agent_profile.name)
            .unwrap();
        let mut fixture = Self {
            space: space_id.into(),
            owner,
            core,
            owner_profile,
            agent_profile,
            agent_device,
            agent_cert,
        };
        fixture.clear(EventBody::SpaceCreated {
            title: "Owner and Agent".into(),
            kind: SpaceKind::Direct,
            privacy: Privacy::EndToEnd,
        });
        fixture.clear(EventBody::MemberAdded {
            identity: fixture.agent_profile.id.clone(),
            role: Role::Member,
        });
        fixture.core.engine.net.profiles.supported = true;
        let (public, signed) = fixture.core.engine.agreement_to_publish().unwrap();
        fixture.owner.agreement_keys_arrived(
            vec![protocol::AgreementKeyRecord {
                identity: fixture.agent_profile.id.clone(),
                public,
                signed,
            }],
            std::slice::from_ref(&fixture.agent_profile.id),
        );
        fixture.owner.create_mls_group(space_id).unwrap();
        let clock = KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap();
        let (space, targets, operation) = fixture.owner.mls_to_claim(Some(clock)).unwrap().unwrap();
        assert_eq!(space, space_id);
        assert_eq!(targets, vec![fixture.agent_profile.id.clone()]);
        let package = fixture
            .core
            .engine
            .device()
            .unwrap()
            .key_packages(1, false)
            .unwrap()
            .remove(0);
        fixture.owner.mls_claimed(
            space_id,
            &operation,
            Ok(vec![KeyPackageRecord {
                identity: fixture.agent_profile.id.clone(),
                device: fixture.agent_device.id(),
                data: package,
            }]),
        );
        for (sha, _) in fixture.owner.pending_uploads(100) {
            fixture.owner.upload_done(&sha);
        }
        let mut handshakes = 0;
        while fixture.owner.outbox_len() > 0 {
            let frames = fixture.owner.outbox_envelopes();
            assert!(!frames.is_empty(), "real protocol outbox must advance");
            for envelope in frames {
                let frame = ordered(&fixture.owner, envelope);
                assert_eq!(fixture.owner.ingest(frame.clone()), Ingest::Confirmed);
                fixture.core.ingest(vec![frame]).unwrap();
                handshakes += 1;
                assert!(handshakes < 12);
            }
        }
        assert!(fixture.core.engine.ready_epoch(space_id).is_some());
        Ok(fixture)
    }

    fn clear(
        &mut self,
        body: EventBody,
    ) -> std::result::Result<String, Box<dyn std::error::Error>> {
        let author = self.owner.net.author.as_ref().unwrap();
        let seen = self.owner.logs.get(&self.space).and_then(SpaceLog::head);
        let event = author.sign_event(&self.space, &new_id("clear")?, 100, seen, body);
        let frame = ordered(&self.owner, Envelope::plain(&event));
        assert_eq!(self.owner.ingest(frame.clone()), Ingest::Applied);
        self.core.ingest(vec![frame.clone()]).unwrap();
        Ok(frame.hash)
    }

    fn encrypted(&mut self, body: EventBody, at_ms: i64) -> String {
        let author = self.owner.net.author.clone().unwrap();
        self.owner
            .append_synced(&self.space, &author, at_ms, body)
            .unwrap();
        self.owner.mls_seal_outbox().unwrap();
        let frames = self.owner.outbox_envelopes();
        assert_eq!(frames.len(), 1);
        let frame = ordered(&self.owner, frames.into_iter().next().unwrap());
        assert_eq!(self.owner.ingest(frame.clone()), Ingest::Confirmed);
        self.core.ingest(vec![frame.clone()]).unwrap();
        frame.hash
    }

    fn grant(&mut self, id: &str, expiry: Option<i64>) -> String {
        self.encrypted(
            EventBody::GrantIssued {
                grant: Grant {
                    id: id.into(),
                    grantor: self.owner_profile.id.clone(),
                    grantee: Some(self.agent_profile.id.clone()),
                    scope: GrantScope::Space(self.space.clone()),
                    capability: Capability::Trust(TrustLevel::Listen),
                    expires_at_ms: expiry,
                },
            },
            200,
        )
    }

    fn message(&mut self, text: &str) -> std::result::Result<String, Box<dyn std::error::Error>> {
        Ok(self.encrypted(
            EventBody::MessagePosted {
                message: new_id("message")?,
                text: text.into(),
                attaches: None,
                reply: None,
                media: None,
                forwarded: false,
            },
            300,
        ))
    }
}

#[test]
fn bounded_discovery_cursor_survives_complete_image() {
    image_discovery_cases::bounded_discovery_cursor_survives_complete_image().unwrap()
}

#[test]
fn prepared_reply_preserves_exact_native_outbox_through_cold_restore_and_confirmation() {
    reply_authority_cases::prepared_reply_preserves_exact_native_outbox_through_cold_restore_and_confirmation().unwrap()
}

#[test]
fn generic_ingest_refuses_colliding_ciphertext_plaintext_pruning_and_changed_header() {
    authenticated_echo_cases::generic_ingest_refuses_colliding_ciphertext_plaintext_pruning_and_changed_header().unwrap()
}

#[test]
fn own_echo_requires_valid_original_inner_and_historical_sealing_copy() {
    authenticated_echo_cases::own_echo_requires_valid_original_inner_and_historical_sealing_copy()
        .unwrap()
}

#[test]
fn lost_sync_marker_or_scope_cannot_downgrade_a_retained_sealed_reply() {
    receipt_recovery_cases::lost_sync_marker_or_scope_cannot_downgrade_a_retained_sealed_reply()
        .unwrap()
}

#[test]
fn exact_old_epoch_echo_survives_real_commit_cold_restore_and_replay() {
    authenticated_echo_cases::exact_old_epoch_echo_survives_real_commit_cold_restore_and_replay()
        .unwrap()
}

#[test]
fn native_receipt_survives_revocation_expiry_and_cold_replay_without_new_effects() {
    receipt_recovery_cases::native_receipt_survives_revocation_expiry_and_cold_replay_without_new_effects().unwrap()
}

#[test]
fn receipt_refuses_different_duplicate_opening_and_missing_original_reference() {
    receipt_recovery_cases::receipt_refuses_different_duplicate_opening_and_missing_original_reference().unwrap()
}

#[test]
fn actual_maximum_text_reference_is_bounded_and_oversized_fields_refuse() {
    receipt_recovery_cases::actual_maximum_text_reference_is_bounded_and_oversized_fields_refuse()
        .unwrap()
}

#[test]
fn receipt_delta_preserves_other_pending_and_refuses_real_new_join_or_row_changes() {
    receipt_recovery_cases::receipt_delta_preserves_other_pending_and_refuses_real_new_join_or_row_changes().unwrap()
}

#[test]
fn later_colliding_echo_invalidates_an_entire_partially_ingested_batch() {
    authenticated_echo_cases::later_colliding_echo_invalidates_an_entire_partially_ingested_batch()
        .unwrap()
}

#[test]
fn preparation_refuses_expired_replaced_revoked_and_reader_authority() {
    reply_authority_cases::preparation_refuses_expired_replaced_revoked_and_reader_authority()
        .unwrap()
}

#[test]
fn concurrent_receipt_prefix_restores_only_the_matching_certified_device() {
    reply_authority_cases::concurrent_receipt_prefix_restores_only_the_matching_certified_device()
        .unwrap()
}

#[test]
fn conflicting_sealed_seen_blocks_cold_restore_with_valid_pending_credentials() {
    receipt_prefix_cases::conflicting_sealed_seen_blocks_cold_restore_with_valid_pending_credentials(
    ).unwrap()
}

#[test]
fn receipt_prefix_keeps_exact_pending_bytes_and_resumes_beyond_64_real_positions() {
    receipt_prefix_cases::receipt_prefix_keeps_exact_pending_bytes_and_resumes_beyond_64_real_positions().unwrap()
}

#[test]
fn receipt_prefix_validates_original_pending_before_reading_unrelated_history() {
    receipt_prefix_cases::receipt_prefix_validates_original_pending_before_reading_unrelated_history(
    ).unwrap()
}

#[test]
fn receipt_prefix_refuses_skipped_duplicate_oversized_and_partially_invalid_batches() {
    receipt_prefix_cases::receipt_prefix_refuses_skipped_duplicate_oversized_and_partially_invalid_batches().unwrap()
}

#[test]
fn preparation_bounds_text_and_never_reseals_an_existing_pending_reply() {
    reply_authority_cases::preparation_bounds_text_and_never_reseals_an_existing_pending_reply()
        .unwrap()
}

#[test]
fn failed_reply_sealing_discards_scratch_and_preserves_committed_image() {
    reply_authority_cases::failed_reply_sealing_discards_scratch_and_preserves_committed_image()
        .unwrap()
}

#[test]
fn discovery_refuses_cursor_outside_verified_history() {
    image_discovery_cases::discovery_refuses_cursor_outside_verified_history().unwrap()
}

#[test]
fn actual_owner_opening_survives_complete_image_and_pins_original_input() {
    image_discovery_cases::actual_owner_opening_survives_complete_image_and_pins_original_input()
        .unwrap()
}

#[test]
fn failed_opening_record_rolls_back_log_and_ratchet_and_discards_workspace() {
    fork_recovery_cases::failed_opening_record_rolls_back_log_and_ratchet_and_discards_workspace()
        .unwrap()
}

#[test]
fn whole_image_keeps_non_openmls_provider_tables_and_real_pending_outbox() {
    image_discovery_cases::whole_image_keeps_non_openmls_provider_tables_and_real_pending_outbox()
        .unwrap()
}

#[test]
fn own_echo_is_context_only_and_mismatching_mls_sender_cannot_refresh() {
    authenticated_echo_cases::own_echo_is_context_only_and_mismatching_mls_sender_cannot_refresh()
        .unwrap()
}

#[test]
fn conflicting_actual_member_checkpoint_parks_live_and_cold_core() {
    fork_recovery_cases::conflicting_actual_member_checkpoint_parks_live_and_cold_core().unwrap()
}

#[test]
fn diverged_member_checkpoint_parks_live_and_cold_core() {
    fork_recovery_cases::diverged_member_checkpoint_parks_live_and_cold_core().unwrap()
}

#[test]
fn sealed_member_seen_ahead_marks_the_fork_and_parks_send() {
    fork_recovery_cases::sealed_member_seen_ahead_marks_the_fork_and_parks_send().unwrap()
}

#[test]
fn no_default_or_optimistic_grant_can_start_a_reply() {
    reply_authority_cases::no_default_or_optimistic_grant_can_start_a_reply().unwrap()
}

#[test]
fn revoke_replacement_reader_and_opaque_history_stop_the_original_reply() {
    reply_authority_cases::revoke_replacement_reader_and_opaque_history_stop_the_original_reply()
        .unwrap()
}

#[test]
fn certified_device_and_owner_proof_are_required_without_root_fallback() {
    reply_authority_cases::certified_device_and_owner_proof_are_required_without_root_fallback()
        .unwrap()
}

#[test]
fn image_rejects_legacy_schema_provider_and_secrets_before_migrations() {
    image_discovery_cases::image_rejects_legacy_schema_provider_and_secrets_before_migrations()
        .unwrap()
}

// Keep the original super::reference/publication targets available to child cases.
use super::{publication, reference};

// Complete cases keep their original annotated entrypoints above.
mod authenticated_echo_cases;
mod fork_recovery_cases;
mod image_discovery_cases;
mod receipt_prefix_cases;
mod receipt_recovery_cases;
mod reply_authority_cases;
mod support;
use support::*;

mod initial_key_packages;

mod outbox_fifo;

#[test]
fn native_image_upgrades_only_exact_previous_outbox_schema_without_changing_signed_queue() {
    outbox_fifo::native_image_upgrades_only_exact_previous_outbox_schema_without_changing_signed_queue().unwrap();
}
