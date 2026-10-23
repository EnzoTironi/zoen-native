//! # protocol — Zoen Sync, the wire protocol (version 4)
//!
//! One WebSocket per device, one protobuf message per binary frame ([`wire`]).
//!
//! ```text
//! client                                         relay
//!   Hello{protocol, caps, identity, device, cert} ──▶
//!                                                ◀── Challenge{nonce, relay, protocol, caps}
//!   Auth{sig(nonce)}                              ──▶
//!                                                ◀── Ready{registered}
//!   Req{Register{profile}}                        ──▶  (first time only)
//!   Sync{cursors}                                 ──▶
//!                                                ◀── Event{..}* SyncDone
//!   Publish{envelope}                             ──▶  sequenced (per-Space order)
//!                                                ◀── Accepted{seq} + Event{..} to every member online
//!   Ephemeral{typing}                             ──▶  forwarded, never stored
//! ```
//!
//! The relay orders and stores; it never rewrites. An envelope is the author's exact
//! signed bytes ([`event_log::content`]) plus the signature: the relay decodes them to route,
//! the client decodes them to show, and nobody encodes them again. Backpressure is the
//! socket's: the relay writes through a bounded queue and disconnects a device that falls
//! too far behind, which then resumes from its cursors.

mod envelope;
mod frames;
pub use envelope::{valid_invite_code, Envelope, Sequenced};
pub use frames::{
    retry_hint, slow_down, AgentAction, AgentRequest, AgentStateRecord, AgreementKeyRecord,
    ClientFrame, Cursor, DeviceCertificate, DeviceRecord, EphemeralKind, ErrorCode, InviteCreated,
    InvitePreview, KeyPackageRecord, Op, Reply, SealedProfile, ServerFrame,
};

pub mod cursor_repair;
pub mod experiments;
pub mod links;
pub mod push;
pub mod runtime_binding;
pub mod sites;
pub mod turn;
pub mod wire;

use event_log::content::{content_hash, decode_body, stub_content, Payload, SignedContent};
pub use event_log::content::{Sealed, SealedKind};
use event_log::{
    event_from_content, signed_content_hash, verify_author, verify_sig, Author, LogError,
};
use types::{Event, EventBody, Identity, IdentityId, Role, Seen, SpaceId};

pub use event_log::profile::DeviceSigned;

/// Version 4 requires explicit relay enrollment when linking a device. Version 3
/// sponsors omitted that enrollment and could report a link that cannot log in.
pub const PROTOCOL_VERSION: u32 = 4;
/// Why a relay refuses a clear event in an end-to-end Space. A client that wrote it before
/// it learned the Space went end-to-end (ADR 0027) seals it and sends it again.
pub const SEAL_REQUIRED: &str = "this space is end-to-end encrypted; seal the event";
/// Under this many single-use key packages the relay asks the device to publish more.
pub const KEY_PACKAGES_LOW: u32 = 8;
/// A connected relay must negotiate this before a client relies on claim replay.
pub const KEY_PACKAGE_CLAIM_CAPABILITY: &str = "key-package-claim-receipts";
/// A claim id is never reused once this retry window expires, even after receipt GC.
pub const KEY_PACKAGE_CLAIM_TTL_MS: i64 = 24 * 60 * 60 * 1000;
pub const KEY_PACKAGE_CLAIM_EXPIRED: &str = "key_package_claim_expired";
/// Why a relay refuses a commit: another one already took its epoch (ADR 0026). The author
/// drops it, applies the winner from the log, and commits again if anything is still owed.
pub const STALE_COMMIT: &str = "stale_epoch: another commit took this epoch";
/// Why a relay refuses a message sealed at an epoch the group has left (a device that
/// sealed before catching up). It waits, catches up, and seals again at the new epoch.
pub const STALE_SEAL: &str = "stale_epoch: sealed at an epoch the group has left";
/// Refuse older clients before authentication, linking or delivering sealed v4 entries.
pub const MIN_PROTOCOL_VERSION: u32 = 4;
/// Domain tag for what devices sign outside the log (login, blob uploads).
/// This stays stable across handshake upgrades; changing it would change signed bytes.
pub const PROTOCOL: &str = "zoen-sync/2";
/// Optional features; each side announces its own and uses the intersection.
pub const AGENT_OWNER_PROOF: &str = "agent_owner_proof_v1";
pub const CAPABILITIES: &[&str] = &[
    "blobs",
    "invites",
    "presence",
    "profiles",
    AGENT_OWNER_PROOF,
    KEY_PACKAGE_CLAIM_CAPABILITY,
    runtime_binding::CAPABILITY,
];

/// A database-clock sample from the authenticated, receipt-capable relay connection.
/// This value is never extrapolated using the device's wall clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyPackageClaimClock(i64);

impl KeyPackageClaimClock {
    pub fn from_server_ms(time: i64) -> Option<Self> {
        (1..=0xFFFF_FFFF_FFFF).contains(&time).then_some(Self(time))
    }

    pub fn milliseconds(self) -> i64 {
        self.0
    }
}

/// Canonical uppercase ULIDs only: no prefixes, aliases or overflowing leading bits.
pub fn key_package_claim_time_ms(operation: &str) -> Option<i64> {
    if operation.len() != 26
        || !matches!(operation.as_bytes().first(), Some(b'0'..=b'7'))
        || operation.bytes().any(|c| c.is_ascii_lowercase())
    {
        return None;
    }
    types::id_time_ms(operation)
}

/// The capabilities both sides have, in our order.
pub fn negotiate(theirs: &[String]) -> Vec<String> {
    CAPABILITIES
        .iter()
        .filter(|c| theirs.iter().any(|t| t == *c))
        .map(|c| c.to_string())
        .collect()
}

/// What the device signs to log in: binds the nonce to this relay and protocol.
pub fn auth_message(nonce: &str, relay: &str) -> Vec<u8> {
    format!("{PROTOCOL}:auth:{relay}:{nonce}").into_bytes()
}

/// What a device signs to upload a blob: binds the content hash and a timestamp to this
/// relay, so a captured header only re-uploads the same bytes for a few minutes.
/// What a device signs to put or delete a chunk of a history transfer (ADR 0045).
/// `op` is "put" or "delete"; `n` and `sha256` are empty for a delete.
pub fn transfer_message(op: &str, transfer: &str, n: &str, sha256: &str, ts_ms: i64) -> Vec<u8> {
    format!("{PROTOCOL}:transfer-{op}:{transfer}:{n}:{sha256}:{ts_ms}").into_bytes()
}

pub fn blob_put_message(sha256: &str, ts_ms: i64, relay: &str) -> Vec<u8> {
    format!("{PROTOCOL}:blob-put:{relay}:{sha256}:{ts_ms}").into_bytes()
}

/// Prefix followed by a 32-byte configuration generation and the encrypted backup.
pub const BACKUP_UPLOAD_MAGIC: &[u8; 8] = b"ZOENBG1\0";

/// Proof that a recovered device controls the key being enrolled for this backup.
pub fn backup_enroll_message(
    identity: &str,
    device: &str,
    cert: &str,
    generation: &str,
) -> Vec<u8> {
    format!("{PROTOCOL}:backup-device-enroll:{identity}:{device}:{cert}:{generation}").into_bytes()
}

/// What a device signs for a backup write (ADR 0046): binds the operation, the identity,
/// the body's hash and a timestamp to this relay.
pub fn backup_message(
    relay: &str,
    identity: &str,
    op: &str,
    body_sha256: &str,
    ts_ms: i64,
) -> Vec<u8> {
    format!("{PROTOCOL}:backup:{relay}:{identity}:{op}:{body_sha256}:{ts_ms}").into_bytes()
}

/// Handles: 3–24 chars, lowercase letters, digits, `_` and `.`; must start with a letter.
pub fn normalize_handle(raw: &str) -> Option<String> {
    let h = raw.trim().trim_start_matches('@').to_lowercase();
    let ok_len = (3..=24).contains(&h.chars().count());
    let ok_chars = h
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
    let ok_start = h
        .chars()
        .next()
        .map(|c| c.is_ascii_lowercase())
        .unwrap_or(false);
    (ok_len && ok_chars && ok_start).then_some(h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use event_log::{chain_hash, Signer, SpaceLog};
    use types::GENESIS_PREV;

    #[test]
    fn recovery_reference_survives_pruning_and_cannot_be_substituted() {
        let author = Author::root(Signer::generate());
        let reference = event_log::recovery::RecoveryRef {
            version: 1,
            epoch: 7,
            blob: "aa".repeat(32),
        };
        let mut sealed = Sealed::new(SealedKind::Commit, 1, b"MLS ciphertext".to_vec());
        sealed.recovery = reference.encode();
        let full = Envelope::sealed(&author, "space", "commit", 1, None, sealed);
        let stub = full.pruned().unwrap();
        assert_eq!(stub.recovery(), full.recovery());
        assert_eq!(stub.wire_hash(), full.wire_hash());
        stub.verify().unwrap();
        let mut changed = stub.parsed.clone();
        let Some(Payload::Sealed(sealed)) = &mut changed.payload else {
            unreachable!()
        };
        sealed.recovery = event_log::recovery::RecoveryRef {
            blob: "bb".repeat(32),
            ..reference
        }
        .encode();
        assert!(Envelope::new(changed.encode(), stub.sig, None, None)
            .unwrap()
            .verify()
            .is_err());
    }

    fn fixture(name: &str) -> Sequenced {
        let data: serde_json::Value = serde_json::from_str(include_str!(
            "../../event-log/testdata/legacy-sealed-v3.json"
        ))
        .unwrap();
        Sequenced::decode(&hex::decode(data[name].as_str().unwrap()).unwrap()).unwrap()
    }

    fn stored_event(ev: &Sequenced) -> Event {
        let mut e = event_from_content(
            ev.env.content.clone(),
            ev.env.sig.clone(),
            ev.env.cert.clone(),
            ev.seq,
            ev.prev.clone(),
            ev.hash.clone(),
        )
        .unwrap();
        e.sealed_wire = ev.env.legacy_pruned_hash().map(str::to_string);
        e
    }

    #[test]
    fn frozen_main_v3_ciphertext_keeps_its_signature_chain_and_outbox_bytes() {
        let ev = fixture("legacy_full");
        ev.env.verify().unwrap();
        assert_eq!(Sequenced::decode(&ev.encode()).unwrap(), ev);
        assert!(
            ev.env.pruned().is_none(),
            "an old signature cannot authenticate a new pruning digest"
        );
        let frame = ClientFrame::Publish {
            env: ev.env.clone(),
        };
        assert_eq!(ClientFrame::decode(&frame.encode()).unwrap(), frame);
        SpaceLog::from_events(
            "sp_upgrade",
            vec![stored_event(&fixture("genesis")), stored_event(&ev)],
        )
        .unwrap();
    }

    #[test]
    fn frozen_legacy_tag_five_stubs_keep_their_authenticated_original_hash() {
        let ev = fixture("legacy_stub");
        let original = fixture("legacy_full");
        ev.env.verify().unwrap();
        assert_eq!(ev.env.wire_hash(), original.env.wire_hash());
        assert_eq!(ev.hash, original.hash);
        assert_eq!(Sequenced::decode(&ev.encode()).unwrap(), ev);
        SpaceLog::from_events(
            "sp_upgrade",
            vec![stored_event(&fixture("genesis")), stored_event(&ev)],
        )
        .unwrap();
        assert!(ClientFrame::decode(
            &ClientFrame::Publish {
                env: ev.env.clone()
            }
            .encode()
        )
        .is_err());
        let mut bad = ev.env;
        bad.sig = "0".repeat(128);
        assert!(bad.verify().is_err());
    }

    #[test]
    fn unauthenticated_legacy_stub_headers_cannot_authorize_recovery() {
        let original = fixture("legacy_stub");
        let mut content = original.env.parsed.clone();
        let Some(Payload::Sealed(sealed)) = &mut content.payload else {
            unreachable!()
        };
        sealed.kind = SealedKind::Commit as i32;
        sealed.recovery = event_log::recovery::RecoveryRef {
            version: 1,
            epoch: 7,
            blob: "aa".repeat(32),
        }
        .encode();
        let grafted = Envelope::new(
            content.encode(),
            original.env.sig.clone(),
            original.env.cert.clone(),
            None,
        )
        .unwrap()
        .with_legacy_pruned(Some(original.env.legacy_pruned_hash().unwrap().into()))
        .unwrap();
        grafted.verify().unwrap();
        assert_eq!(grafted.sig, original.env.sig);
        assert_eq!(grafted.wire_hash(), original.env.wire_hash());
        assert_eq!(grafted.sealed_kind(), Some(SealedKind::Commit));
        assert!(grafted.recovery().is_none());
        let modified = Sequenced {
            env: grafted,
            ..original.clone()
        };
        SpaceLog::from_events(
            "sp_upgrade",
            vec![stored_event(&fixture("genesis")), stored_event(&modified)],
        )
        .unwrap();
        assert_eq!(Sequenced::decode(&modified.encode()).unwrap(), modified);
    }

    #[test]
    fn frozen_unversioned_m2_hashes_keep_their_chain_and_pruning_proof() {
        let ev = fixture("unversioned_full");
        let stub = fixture("unversioned_stub");
        ev.env.verify().unwrap();
        stub.env.verify().unwrap();
        assert_eq!(ev.env.pruned().unwrap(), stub.env);
        assert_eq!(ev.env.wire_hash(), stub.env.wire_hash());
        for entry in [ev, stub] {
            SpaceLog::from_events(
                "sp_upgrade",
                vec![stored_event(&fixture("genesis")), stored_event(&entry)],
            )
            .unwrap();
        }
    }

    #[test]
    fn versioned_stubs_authenticate_unknown_fields_and_refuse_legacy_markers() {
        let author = Author::root(event_log::Signer::from_secret(&[37; 32]));
        let env = Envelope::sealed(
            &author,
            "sp",
            "message",
            3,
            None,
            Sealed::new(SealedKind::Application, 1, vec![1, 2, 3]),
        );
        let mut bytes = env.content().to_vec();
        // A newer client's unknown length-delimited field 127 must remain signed.
        bytes.extend_from_slice(&[0xfa, 0x07, 0x03, b'n', b'e', b'w']);
        let sig = author.key.sign(&event_log::content::signature_message(
            &bytes,
            &event_log::content::signed_hash(&bytes),
        ));
        let full = Envelope::new(bytes, sig, None, None).unwrap();
        full.verify().unwrap();
        assert_eq!(full.parsed.v, event_log::content::SEALED_CONTENT_VERSION);
        let stub = full.pruned().unwrap();
        stub.verify().unwrap();
        assert_eq!(stub.wire_hash(), full.wire_hash());
        assert!(stub
            .content
            .ends_with(&[0xfa, 0x07, 0x03, b'n', b'e', b'w']));
        assert!(stub
            .clone()
            .with_legacy_pruned(Some(full.wire_hash()))
            .is_none());
        let mut changed = stub.content.clone();
        *changed.last_mut().unwrap() = b'x';
        assert!(Envelope::new(changed, stub.sig.clone(), None, None)
            .unwrap()
            .verify()
            .is_err());
        assert_eq!(
            chain_hash("sp", 0, GENESIS_PREV, &full.wire_hash()),
            chain_hash("sp", 0, GENESIS_PREV, &stub.wire_hash())
        );
    }

    #[test]
    fn version_four_signatures_cannot_be_downgraded_to_legacy_tag_five_stubs() {
        let author = Author::root(event_log::Signer::from_secret(&[39; 32]));
        let full = Envelope::sealed(
            &author,
            "sp",
            "authenticated-client-id",
            3,
            None,
            Sealed::new(SealedKind::Application, 1, vec![1, 2, 3]),
        );
        full.verify().unwrap();
        let original_hash = full.wire_hash();
        let mut forged = full.parsed.clone();
        forged.v = 3;
        forged.client_id = "forged-client-id".to_string();
        let Some(Payload::Sealed(sealed)) = &mut forged.payload else {
            panic!("sealed fixture");
        };
        sealed.data.clear();
        sealed.data_hash.clear();
        let forged = Envelope::new(forged.encode(), full.sig.clone(), None, None)
            .unwrap()
            .with_legacy_pruned(Some(original_hash.clone()))
            .unwrap();
        assert!(forged.verify().is_err());

        // The persisted Event path must reject the same opaque legacy hash replay.
        let hash = chain_hash("sp", 0, GENESIS_PREV, &original_hash);
        let mut event = event_from_content(
            forged.content,
            forged.sig,
            None,
            0,
            GENESIS_PREV.to_string(),
            hash,
        )
        .unwrap();
        event.sealed_wire = Some(original_hash);
        assert!(verify_author(&event).is_err());
    }

    #[test]
    fn unsigned_invite_is_bounded_before_wire_admission() {
        let mut env = fixture("genesis").env;
        let signed_size = env.stored_len();
        for code in ["0123456789", "abcdefghjk"] {
            env.invite = Some(code.into());
            assert!(env.valid_invite());
            let wire = ClientFrame::Publish { env: env.clone() }.encode();
            assert!(ClientFrame::decode(&wire).is_ok());
            assert_eq!(env.retained_len(), signed_size + 10);
        }
        env.invite = Some("x".repeat(1024 * 1024 - 1024));
        assert!(
            env.verify().is_ok(),
            "the signature doesn't cover the invite"
        );
        let wire = ClientFrame::Publish { env: env.clone() }.encode();
        assert!(wire.len() < 1024 * 1024, "the attack fits a session frame");
        assert!(!env.valid_invite());
        assert!(ClientFrame::decode(&wire).is_err());
        assert_eq!(env.retained_len(), signed_size + 1024 * 1024 - 1024);
    }

    #[test]
    fn handles_are_normalized() {
        assert_eq!(normalize_handle("@Enzo.T").as_deref(), Some("enzo.t"));
        assert_eq!(normalize_handle("1abc"), None);
        assert_eq!(normalize_handle("ab"), None);
        assert_eq!(normalize_handle("a b c"), None);
    }

    #[test]
    fn claim_operations_are_canonical_ulids() {
        let operation = "01K75MYBPQ0000000000000001";
        assert!(key_package_claim_time_ms(operation).is_some());
        for invalid in [
            operation.to_lowercase(),
            format!("prefix-{operation}"),
            "81K75MYBPQ0000000000000001".into(),
            "01K75MYBPQ000000000000000I".into(),
        ] {
            assert_eq!(key_package_claim_time_ms(&invalid), None, "{invalid}");
        }
        assert_eq!(negotiate(&[]), Vec::<String>::new());
        assert_eq!(
            negotiate(&[KEY_PACKAGE_CLAIM_CAPABILITY.into()]),
            vec![KEY_PACKAGE_CLAIM_CAPABILITY]
        );
    }

    #[test]
    fn capabilities_are_the_intersection() {
        assert_eq!(
            negotiate(&["presence".into(), "telepathy".into(), "blobs".into()]),
            vec!["blobs", "presence"]
        );
        assert!(negotiate(&[]).is_empty());
        assert_eq!(
            negotiate(&[AGENT_OWNER_PROOF.into()]),
            vec![AGENT_OWNER_PROOF]
        );
    }
}
pub mod provider_auth;
