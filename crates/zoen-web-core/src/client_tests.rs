use super::*;
#[path = "client_tests/app_documents.rs"]
mod app_documents;
#[path = "client_tests/artifact_discussions.rs"]
mod artifact_discussions;
#[path = "client_tests/attachments.rs"]
mod attachments;
#[path = "client_tests/crypto_interop.rs"]
mod crypto_interop;
#[path = "client_tests/cursor_repair.rs"]
mod cursor_repair;
#[path = "client_tests/page_documents.rs"]
mod page_documents;
#[path = "client_tests/projection.rs"]
mod projection;
#[path = "client_tests/receipts.rs"]
mod receipts;
#[path = "client_tests/voice.rs"]
mod voice;

use event_log::{Author, Signer as NativeSigner};
use mls::Device;
use openmls_traits::{signatures::SignerError, types::SignatureScheme};

#[path = "client_tests_drawings.rs"]
mod drawings;

#[derive(Clone)]
pub(super) struct HostSigner(pub(super) NativeSigner);
impl Signer for HostSigner {
    fn sign(&self, payload: &[u8]) -> Result<Vec<u8>, SignerError> {
        hex::decode(self.0.sign(payload)).map_err(|_| SignerError::SigningError)
    }
    fn signature_scheme(&self) -> SignatureScheme {
        mls::SUITE.signature_algorithm()
    }
}

pub(super) fn account(identity: &NativeSigner, device: &NativeSigner) -> LinkedAccount {
    LinkedAccount {
        identity: identity.id(),
        device: device.id(),
        cert: identity.sign(&event_log::device_cert_message(&device.id())),
        name: "Fixture".into(),
        handle: "fixture".into(),
        relay: "http://127.0.0.1:9000".into(),
        primary: identity.id(),
        primary_hpke: String::new(),
        hpke_secret: String::new(),
        link_secret: String::new(),
        agreement_secret: String::new(),
        profile_key: String::new(),
        profile_key_version: 0,
        profile_seen_version: 0,
        profile_fields: String::new(),
    }
}

pub(super) fn sequence(log: &mut SpaceLog, env: Envelope) -> Vec<u8> {
    let outer = event_log::event_from_content(
        env.content().to_vec(),
        env.sig.clone(),
        env.cert.clone(),
        0,
        String::new(),
        String::new(),
    )
    .unwrap();
    let event = log.sequence(outer);
    ServerFrame::Event {
        ev: Sequenced {
            seq: event.seq,
            prev: event.prev.clone(),
            hash: event.hash.clone(),
            env,
        },
    }
    .encode()
}

pub(super) fn clear(log: &mut SpaceLog, author: &Author, body: EventBody, now: i64) -> Vec<u8> {
    let event = author.sign_event(log.space(), &types::new_ulid(now), now, log.head(), body);
    sequence(log, Envelope::plain(&event))
}

fn sealed(
    log: &mut SpaceLog,
    author: &Author,
    kind: SealedKind,
    data: Vec<u8>,
    now: i64,
) -> Vec<u8> {
    let (content, sig) = author.sign_sealed(
        log.space(),
        &types::new_ulid(now),
        now,
        log.head().as_ref(),
        Sealed::new(kind, SUITE_ID, data),
    );
    sequence(
        log,
        Envelope::new(content, sig, author.cert.clone(), None).unwrap(),
    )
}

#[test]
fn native_sqlite_welcome_messages_saved_reload_and_removal_use_shared_mls() {
    crypto_interop::native_sqlite_welcome_messages_saved_reload_and_removal_use_shared_mls();
}

#[test]
fn invitation_queues_a_certified_self_add_with_the_original_code_and_no_plain_message_fallback() {
    crypto_interop::invitation_queues_a_certified_self_add_with_the_original_code_and_no_plain_message_fallback();
}

#[test]
fn native_voice_replies_versions_and_reverts_stay_on_the_original_post() {
    voice::native_voice_replies_versions_and_reverts_stay_on_the_original_post();
}

#[test]
fn native_forwarded_voice_is_one_destination_only_message_after_receive_and_saved_reload() {
    voice::native_forwarded_voice_is_one_destination_only_message_after_receive_and_saved_reload();
}

fn web_fixture() -> (Client<HostSigner>, Author, Author, SpaceLog, String) {
    let identity = NativeSigner::from_secret(&[41; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[42; 32]));
    let author = Author::device(&identity, NativeSigner::from_secret(&[43; 32]));
    let other = Author::device(
        &NativeSigner::from_secret(&[44; 32]),
        NativeSigner::from_secret(&[45; 32]),
    );
    let client = Client::new(account(&identity, &host.0), host, "").unwrap();
    let mut log = SpaceLog::new("sp_web_receipts");
    clear(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Fixture".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::EndToEnd,
        },
        1,
    );
    clear(
        &mut log,
        &author,
        EventBody::MessagePosted {
            message: "msg_fixture".into(),
            text: "Oi".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
        2,
    );
    let target = log.events().last().unwrap().hash.clone();
    (client, author, other, log, target)
}

fn annotate(log: &mut SpaceLog, author: &Author, target: &str, note: types::Annotation) {
    let now = log.next_seq() as i64 + 10;
    clear(
        log,
        author,
        EventBody::MessageAnnotated {
            target: target.into(),
            note,
        },
        now,
    );
}

#[test]
fn conversation_metadata_comes_from_logs_and_only_my_personal_log_controls_preferences() {
    projection::conversation_metadata_comes_from_logs_and_only_my_personal_log_controls_preferences(
    );
}

#[test]
fn web_reaction_participants_replace_remove_and_ignore_invalid_reactions() {
    projection::web_reaction_participants_replace_remove_and_ignore_invalid_reactions();
}

#[test]
fn web_reaction_submission_rejects_blank_and_oversized_utf8_before_encryption() {
    projection::web_reaction_submission_rejects_blank_and_oversized_utf8_before_encryption();
}

fn receipt_marks(frames: &[Vec<u8>]) -> Vec<(&'static str, u64)> {
    frames
        .iter()
        .filter_map(|frame| match ClientFrame::decode(frame).unwrap() {
            ClientFrame::Ephemeral {
                kind: protocol::EphemeralKind::Delivered { seq },
                ..
            } => Some(("delivered", seq)),
            ClientFrame::Ephemeral {
                kind: protocol::EphemeralKind::Read { seq },
                ..
            } => Some(("read", seq)),
            _ => None,
        })
        .collect()
}

#[test]
fn web_receipts_replay_after_saved_unsent_frames_and_each_reconnect() {
    receipts::web_receipts_replay_after_saved_unsent_frames_and_each_reconnect();
}

fn document_fixture() -> (Client<HostSigner>, Author, SpaceLog) {
    let identity = NativeSigner::from_secret(&[71; 32]);
    let host = HostSigner(NativeSigner::from_secret(&[72; 32]));
    let author = Author::device(&identity, host.0.clone());
    let mut client = Client::new(account(&identity, &host.0), host, "").unwrap();
    let mut log = SpaceLog::new("sp_web_documents");
    let created = clear(
        &mut log,
        &author,
        EventBody::SpaceCreated {
            title: "Documentos compartilhados".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
        1,
    );
    client.receive(&created, 2).unwrap();
    client.receive(&ServerFrame::SyncDone.encode(), 3).unwrap();
    (client, author, log)
}

fn doc(client: &mut Client<HostSigner>, command: serde_json::Value, now: i64) -> DocumentResponse {
    client.document_request(&command.to_string(), now).unwrap()
}

fn confirm_documents(
    client: &mut Client<HostSigner>,
    log: &mut SpaceLog,
    update: Update,
    now: i64,
) {
    for frame in update.frames {
        if let ClientFrame::Publish { env } = ClientFrame::decode(&frame).unwrap() {
            let frame = sequence(log, env);
            client.receive(&frame, now).unwrap();
        }
    }
}

#[test]
fn browser_new_document_notes_follow_language_without_rewriting_received_history() {
    page_documents::browser_new_document_notes_follow_language_without_rewriting_received_history();
}

#[test]
fn browser_pages_share_native_codec_persist_drafts_and_replay_receipts() {
    page_documents::browser_pages_share_native_codec_persist_drafts_and_replay_receipts();
}

#[test]
fn browser_stale_page_editor_merges_unseen_blocks_and_restores_as_a_new_change() {
    page_documents::browser_stale_page_editor_merges_unseen_blocks_and_restores_as_a_new_change();
}

#[test]
fn browser_large_page_upload_barrier_survives_reload_and_missing_download_waits() {
    page_documents::browser_large_page_upload_barrier_survives_reload_and_missing_download_waits();
}

#[test]
fn browser_plans_preserve_large_cents_and_refuse_stale_writes_and_undo() {
    app_documents::browser_plans_preserve_large_cents_and_refuse_stale_writes_and_undo();
}

#[test]
fn browser_app_install_uses_request_locale_and_preserves_signed_history() {
    app_documents::browser_app_install_uses_request_locale_and_preserves_signed_history();
}

#[test]
fn browser_miniapps_use_shared_rules_item_scoped_grants_and_confirmations() {
    app_documents::browser_miniapps_use_shared_rules_item_scoped_grants_and_confirmations();
}

#[test]
fn browser_closed_chat_posts_queue_offline_and_never_publish_plaintext_after_encryption() {
    receipts::browser_closed_chat_posts_queue_offline_and_never_publish_plaintext_after_encryption(
    );
}

#[test]
fn browser_file_cards_keep_the_native_message_identity_for_deletion_and_replies() {
    receipts::browser_file_cards_keep_the_native_message_identity_for_deletion_and_replies();
}

#[test]
fn browser_directory_never_trusts_person_fields_or_reassigns_a_proven_agent_owner() {
    projection::browser_directory_never_trusts_person_fields_or_reassigns_a_proven_agent_owner();
}

#[test]
fn browser_typing_is_connection_local_and_requires_a_current_writing_member() {
    projection::browser_typing_is_connection_local_and_requires_a_current_writing_member();
}

#[test]
fn browser_task_checkbox_draft_survives_reread_and_cold_journal_restore() {
    app_documents::browser_task_checkbox_draft_survives_reread_and_cold_journal_restore();
}

#[test]
fn received_app_owned_documents_keep_manifest_history_without_becoming_file_cards() {
    app_documents::received_app_owned_documents_keep_manifest_history_without_becoming_file_cards();
}

#[test]
fn actual_public_receive_emits_delivery_and_mark_read_replays_after_serialized_snapshot_reload() {
    receipts::actual_public_receive_emits_delivery_and_mark_read_replays_after_serialized_snapshot_reload();
}

#[test]
fn actual_signed_removal_discards_new_receipts_and_rejoin_cannot_authorize_the_discarded_arrival() {
    receipts::actual_signed_removal_discards_new_receipts_and_rejoin_cannot_authorize_the_discarded_arrival();
}
#[test]
fn actual_early_receipt_waits_for_confirmed_signed_membership_and_connection_buffer_is_bounded() {
    receipts::actual_early_receipt_waits_for_confirmed_signed_membership_and_connection_buffer_is_bounded();
}

fn attachment_frame(sealed: &crate::media::Sealed) -> Vec<u8> {
    let uploads = sealed
        .uploads
        .iter()
        .map(|upload| serde_json::json!({"blob":upload.blob,"len":upload.sealed.len()}))
        .collect::<Vec<_>>();
    let head =
        serde_json::to_vec(&serde_json::json!({"doc":sealed.doc,"uploads":uploads})).unwrap();
    let mut frame = (head.len() as u32).to_le_bytes().to_vec();
    frame.extend(head);
    for upload in &sealed.uploads {
        frame.extend(&upload.sealed);
    }
    frame
}

fn issued_attachment(client: &mut Client<HostSigner>, space: &str) -> serde_json::Value {
    let producer = client.attachment_producer(r#"{"op":"begin"}"#).unwrap();
    let mutation = client
        .attachment_producer(&serde_json::json!({"op":"issue","producer_id":producer}).to_string())
        .unwrap();
    serde_json::json!({"space_id":space,"mutation_id":mutation,"caption":"  Página 🗂️\nExact bytes.  ","reply_to":null,"thread":false})
}

#[test]
fn attachment_intent_persists_both_signed_events_and_real_ciphertext_before_any_publish() {
    attachments::attachment_intent_persists_both_signed_events_and_real_ciphertext_before_any_publish();
}

#[test]
fn attachment_intent_lost_response_reencrypt_retry_and_ui_ack_have_one_canonical_effect() {
    attachments::attachment_intent_lost_response_reencrypt_retry_and_ui_ack_have_one_canonical_effect();
}

#[test]
fn attachment_intent_gaps_drop_unissued_and_retired_leases_fail_closed_without_lifetime_cap() {
    attachments::attachment_intent_gaps_drop_unissued_and_retired_leases_fail_closed_without_lifetime_cap();
}

#[test]
fn attachment_intent_faults_preserve_existing_outbox_and_do_not_consume_issued_sequence() {
    attachments::attachment_intent_faults_preserve_existing_outbox_and_do_not_consume_issued_sequence();
}

#[test]
fn attachment_intent_authentic_chunk_length_lies_preserve_snapshot_and_issued_sequence() {
    attachments::attachment_intent_authentic_chunk_length_lies_preserve_snapshot_and_issued_sequence();
}

#[test]
fn attachment_intent_authentic_thumbnail_length_lies_preserve_snapshot_and_issued_sequence() {
    attachments::attachment_intent_authentic_thumbnail_length_lies_preserve_snapshot_and_issued_sequence();
}
