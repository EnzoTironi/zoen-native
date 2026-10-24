//! Encrypted profiles, engine side (ADR 0016).
//!
//! - **Mine:** a random profile key and the fields it seals live in `profile_keys` under my
//!   own id. Every edit bumps the version; the sealed copy goes to the relay, signed by
//!   this device.
//! - **Shares:** for each person I share a relay Space with, the current key is sealed to
//!   their published X25519 agreement key and posted as `ProfileKeyShared` in that Space.
//!   One share per person per key; a new key (blocking someone) means new shares for
//!   everyone else.
//! - **Theirs:** a share addressed to me stores their key; their sealed profile is then
//!   fetched, checked against their identity and opened. Without a key, a person is their
//!   @handle.

use std::collections::HashSet;

use event_log::profile::{
    open_profile, open_share, seal_profile, seal_share, share_context, sign_agreement, sign_upload,
    verify_agreement, verify_upload, AgreementKey, DeviceSigned, ProfileFields, ProfileKey,
    ProfilePhoto, SealedShare, MAX_CIPHERTEXT,
};
use event_log::profile_sync::{Observe, OwnProfileSync};
use protocol::{AgreementKeyRecord, SealedProfile};
use sha2::{Digest, Sha256};
use types::*;

use crate::engine::{now_ms, Engine, R};
use crate::i18n::t;
use crate::media::Wanted;
use crate::CoreError;

pub(crate) const VAULT_AGREEMENT: &str = "zoen.agreement.v1";
const META_UPLOADED: &str = "profile:uploaded"; // read once during migration
const META_OWN_SYNC: &str = "profile:own-sync:v1";

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct OwnJournal {
    sync: OwnProfileSync,
    upload: Option<StoredUpload>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredUpload {
    version: u64,
    ciphertext: Vec<u8>,
    device: String,
    cert: String,
    sig: String,
}

impl StoredUpload {
    fn sealed(&self, identity: &str) -> SealedProfile {
        SealedProfile {
            identity: identity.into(),
            version: self.version,
            ciphertext: self.ciphertext.clone(),
            signed: DeviceSigned {
                device: self.device.clone(),
                cert: self.cert.clone(),
                sig: self.sig.clone(),
            },
        }
    }
}
const META_AGREEMENT: &str = "agreement:published";
/// Shares per event until MLS carries them (each is ~200 bytes of hex).
const SHARES_PER_EVENT: usize = 200;
const MAX_NAME: usize = 64;
const MAX_BIO: usize = 280;
use event_log::profile::MAX_PHOTO_BYTES;
const BATCH: usize = 500;

/// Profile bookkeeping for the network task.
#[derive(Default)]
pub struct ProfileNet {
    pub agreement: Option<AgreementKey>,
    /// People whose agreement key a share is waiting for.
    pub need_agreement: HashSet<IdentityId>,
    /// Asked this session; they haven't published one (agents, old clients).
    pub no_agreement: HashSet<IdentityId>,
    /// Key holders whose sealed profile should be (re)fetched.
    pub need_profiles: HashSet<IdentityId>,
    pub shares_dirty: bool,
    /// The relay speaks the `profiles` capability (older relays: no profile traffic).
    pub supported: bool,
    /// Who already holds my key at a version: rebuilt from the logs when the version moves.
    shared: Option<(u64, HashSet<IdentityId>)>,
    own_refreshed: bool,
    fetching: HashSet<IdentityId>,
    refetch: HashSet<IdentityId>,
}

impl ProfileNet {
    pub(crate) fn forget_shared(&mut self) {
        self.shared = None;
    }
}

/// How `update_my_profile` treats the photo.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum PhotoChange {
    Keep,
    Remove,
    Set { bytes: Vec<u8>, mime: String },
}

/// A person's profile as this device can see it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct ProfileDto {
    pub identity_id: String,
    pub handle: String,
    /// `None` when this device holds no key for them (not a contact): show `@handle`.
    pub name: Option<String>,
    pub bio: Option<String>,
    /// The photo's sha256; read using `profile_photo(identity, expected_sha256)`.
    pub photo_sha256: Option<String>,
    pub photo_ready: bool,
    pub photo_mime: Option<String>,
    /// The profile version shown (0 = none readable yet).
    pub version: u64,
    pub tint_hex: String,
    pub is_me: bool,
    pub blocked: bool,
}

/// A bounded disposable view of the exact photo currently readable for an identity.
#[derive(Clone, uniffi::Record)]
pub struct ProfilePhotoBytes {
    pub identity_id: String,
    pub sha256: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

fn invalid(reason: String) -> CoreError {
    CoreError::Invalid { reason }
}

mod editing;
mod incoming;
mod journal;
mod publication;
mod sharing;
mod views;

#[cfg(test)]
mod own_sync_tests {
    use super::*;
    use event_log::{Author, Signer};

    fn fixture() -> (Engine, Signer) {
        fixture_at(":memory:")
    }

    fn fixture_at(path: &str) -> (Engine, Signer) {
        let identity = Signer::from_secret(&[71; 32]);
        let author = Author::device(&identity, Signer::from_secret(&[72; 32]));
        let mut engine = Engine::open(path).unwrap();
        engine.me = Some(identity.id());
        engine.store.set_meta("me", &identity.id()).unwrap();
        engine.net.author = Some(author);
        engine.init_profile(&identity.id(), "Original").unwrap();
        engine.profiles_session_start(true);
        engine.sealed_profiles_arrived(Vec::new(), &[identity.id()]);
        let upload = engine.profile_to_upload().unwrap();
        engine.profile_upload_answered(upload.version, Ok(()));
        (engine, identity)
    }

    fn remote(
        engine: &Engine,
        identity: &Signer,
        version: u64,
        name: &str,
        rotated: bool,
    ) -> std::result::Result<SealedProfile, Box<dyn std::error::Error>> {
        let author = Author::device(identity, Signer::from_secret(&[92; 32]));
        let key = if rotated {
            ProfileKey::generate()?
        } else {
            engine.my_fields(&identity.id()).unwrap().0
        };
        let ciphertext = seal_profile(
            &key,
            &identity.id(),
            version,
            &ProfileFields {
                name: name.into(),
                ..Default::default()
            },
        )?;
        Ok(SealedProfile {
            identity: identity.id(),
            version,
            signed: sign_upload(&author, version, &ciphertext).unwrap(),
            ciphertext,
        })
    }

    #[test]
    fn clean_same_identity_update_adopts_without_republication() {
        let (mut engine, identity) = fixture();
        let (photo, _) = ProfilePhoto::seal(b"synthetic peer photo", "image/jpeg").unwrap();
        let fields = ProfileFields {
            name: "Other device".into(),
            photo: Some(photo.clone()),
            ..Default::default()
        };
        let mut incoming = remote(&engine, &identity, 2, "Other device", false).unwrap();
        incoming.ciphertext = seal_profile(
            &engine.my_fields(&identity.id()).unwrap().0,
            &identity.id(),
            2,
            &fields,
        )
        .unwrap();
        incoming.signed = sign_upload(
            &Author::device(&identity, Signer::from_secret(&[92; 32])),
            2,
            &incoming.ciphertext,
        )
        .unwrap();
        engine.profile_changed(&identity.id());
        assert!(engine.take_need_profiles().contains(&identity.id()));
        assert_eq!(
            engine.sealed_profiles_arrived(vec![incoming], &[identity.id()]),
            vec![identity.id()]
        );
        assert_eq!(
            engine.my_fields(&identity.id()).unwrap().3.name,
            "Other device"
        );
        assert_eq!(
            engine.my_fields(&identity.id()).unwrap().3.photo,
            Some(photo.clone())
        );
        assert!(engine
            .wanted_profile_photos()
            .iter()
            .any(
                |wanted| wanted.profile.as_deref() == Some(identity.id().as_str())
                    && wanted.plain == photo.sha256
                    && wanted.blob == photo.blob
            ));
        assert!(engine.profile_to_upload().is_none());
        let sync = engine.own_profile_journal().unwrap().sync;
        assert_eq!(sync.published, 2);
        assert_eq!(sync.pending, None);
    }

    #[test]
    fn incoming_profile_ciphertext_obeys_existing_size_ceiling_before_cache() {
        let (mut engine, identity) = fixture();
        let bytes = vec![17; MAX_PHOTO_BYTES + 1];
        let (key, sealed) = crate::media::seal(&bytes).unwrap();
        let photo = ProfilePhoto {
            sha256: hex::encode(Sha256::digest(&bytes)),
            blob: hex::encode(Sha256::digest(&sealed)),
            key: hex::encode(key),
            mime: "image/jpeg".into(),
        };
        assert!(photo.valid(), "well-formed authenticated reference");
        assert!(
            photo.open(&sealed).is_none(),
            "existing profile size policy"
        );
        let fields = ProfileFields {
            name: "Synthetic second-device profile".into(),
            photo: Some(photo.clone()),
            ..Default::default()
        };
        let mut incoming = remote(&engine, &identity, 2, "Synthetic", false).unwrap();
        incoming.ciphertext = seal_profile(
            &engine.my_fields(&identity.id()).unwrap().0,
            &identity.id(),
            2,
            &fields,
        )
        .unwrap();
        incoming.signed = sign_upload(
            &Author::device(&identity, Signer::from_secret(&[92; 32])),
            2,
            &incoming.ciphertext,
        )
        .unwrap();
        assert_eq!(
            engine.sealed_profiles_arrived(vec![incoming], &[identity.id()]),
            [identity.id()]
        );
        let wanted = engine
            .wanted_profile_photos()
            .into_iter()
            .find(|wanted| wanted.plain == photo.sha256)
            .expect("actual authenticated photo demand");
        let profile_before = engine.my_fields(&identity.id()).unwrap().3;
        let events_before = engine.store.event_count().unwrap();
        assert!(
            !engine.media_arrived(&wanted, &sealed),
            "oversized valid ciphertext must not enter the media cache"
        );
        assert_eq!(
            wanted.maximum_sealed_bytes,
            Some(MAX_PHOTO_BYTES as u64 + 40),
            "the actual download demand is bounded before HTTP reads"
        );
        assert!(engine.media(&photo.sha256).unwrap().is_none());
        assert!(engine.store.media_key(&photo.sha256).unwrap().is_none());
        assert_eq!(engine.my_fields(&identity.id()).unwrap().3, profile_before);
        assert_eq!(engine.store.event_count().unwrap(), events_before);
    }

    #[test]
    fn concurrent_self_profile_preserves_pending_and_exact_retry_across_network_reset() {
        let (mut engine, identity) = fixture();
        engine
            .update_my_profile("Local pending", "bio", PhotoChange::Keep)
            .unwrap();
        let old = engine.profile_to_upload().unwrap();
        let incoming = remote(&engine, &identity, 4, "Other device", false).unwrap();
        engine.sealed_profiles_arrived(vec![incoming.clone()], &[identity.id()]);
        assert_eq!(
            engine.my_fields(&identity.id()).unwrap().3.name,
            "Local pending"
        );
        let upload = engine.profile_to_upload().unwrap();
        assert_eq!(upload.version, 5);
        engine.profile_upload_answered(old.version, Ok(()));
        assert_eq!(engine.own_profile_journal().unwrap().sync.pending, Some(5));
        engine.net.profiles = ProfileNet::default();
        engine.profiles_session_start(true);
        engine.sealed_profiles_arrived(vec![incoming], &[identity.id()]);
        assert_eq!(engine.profile_to_upload(), Some(upload.clone()));
        engine.sealed_profiles_arrived(vec![upload], &[identity.id()]);
        assert!(engine.profile_to_upload().is_none());
    }

    #[test]
    fn pending_upload_bytes_survive_actual_sqlite_close_and_reopen() {
        let path = std::env::temp_dir().join(format!(
            "zoen-profile-sync-{}-{}.sqlite",
            std::process::id(),
            types::new_ulid(now_ms()).unwrap()
        ));
        let path = path.to_str().unwrap();
        let (mut engine, identity) = fixture_at(path);
        engine
            .update_my_profile("Cold pending", "bio", PhotoChange::Keep)
            .unwrap();
        let upload = engine.profile_to_upload().unwrap();
        drop(engine);
        let mut reopened = Engine::open(path).unwrap();
        reopened.net.author = Some(Author::device(&identity, Signer::from_secret(&[72; 32])));
        reopened.profiles_session_start(true);
        let baseline = remote(&reopened, &identity, 1, "Original", false).unwrap();
        reopened.sealed_profiles_arrived(vec![baseline], &[identity.id()]);
        assert_eq!(reopened.profile_to_upload(), Some(upload));
        drop(reopened);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{path}-wal"));
        let _ = std::fs::remove_file(format!("{path}-shm"));
    }

    #[test]
    fn unknown_rotation_never_republishes_an_obsolete_profile_key() {
        let (mut engine, identity) = fixture();
        engine
            .update_my_profile("Local pending", "bio", PhotoChange::Keep)
            .unwrap();
        let (_, _, _, fields) = engine.my_fields(&identity.id()).unwrap();
        engine
            .store
            .set_profile_fields(&identity.id(), 100, &fields.to_bytes())
            .unwrap();
        engine.mark_profile_intent(100).unwrap();
        let incoming = remote(&engine, &identity, 8, "Rotated", true).unwrap();
        engine.sealed_profiles_arrived(vec![incoming], &[identity.id()]);
        assert!(engine.own_profile_journal().unwrap().sync.key_unavailable());
        let old = remote(&engine, &identity, 2, "Old key", false).unwrap();
        engine.sealed_profiles_arrived(vec![old], &[identity.id()]);
        engine.profile_upload_answered(2, Ok(()));
        engine.profile_upload_answered(100, Ok(()));
        let newer_old_key =
            remote(&engine, &identity, 101, "Old key published higher", false).unwrap();
        engine.sealed_profiles_arrived(vec![newer_old_key], &[identity.id()]);
        let state = engine.own_profile_journal().unwrap().sync;
        assert_eq!(state.blocked_at_version, Some(8));
        assert_eq!(state.pending, Some(100));
        assert!(engine.profile_to_upload().is_none());
        assert!(!engine.profiles_settled());
        assert_eq!(
            engine.my_fields(&identity.id()).unwrap().3.name,
            "Local pending"
        );
    }
    #[test]
    fn maximum_remote_version_cannot_wrap_text_photo_or_block_rotation() {
        let (mut engine, identity) = fixture();
        let (_, _, _, fields) = engine.my_fields(&identity.id()).unwrap();
        engine
            .store
            .set_profile_fields(&identity.id(), u64::MAX, &fields.to_bytes())
            .unwrap();
        engine
            .save_own_profile_journal(&OwnJournal {
                sync: OwnProfileSync::clean(u64::MAX),
                upload: None,
            })
            .unwrap();
        let before = engine.my_fields(&identity.id()).unwrap();
        let journal = engine.store.meta(META_OWN_SYNC).unwrap();
        assert!(engine.set_profile_text("Overflow", "bio").is_err());
        assert!(engine
            .update_my_profile("Overflow", "bio", PhotoChange::Keep)
            .is_err());
        assert!(engine
            .block_person(&Signer::from_secret(&[93; 32]).id())
            .is_err());
        assert_eq!(engine.my_fields(&identity.id()).unwrap(), before);
        assert_eq!(engine.store.meta(META_OWN_SYNC).unwrap(), journal);
        assert!(engine.blocked_people().unwrap().is_empty());
    }
    #[test]
    fn invalid_own_lookup_preserves_fields_intent_and_cannot_release_upload() {
        let (mut engine, identity) = fixture();
        engine
            .update_my_profile("Local pending", "bio", PhotoChange::Keep)
            .unwrap();
        engine.profile_changed(&identity.id());
        let asked = engine.take_need_profiles();
        let before = engine.my_fields(&identity.id()).unwrap();
        let journal = engine.store.meta(META_OWN_SYNC).unwrap();
        let mut forged = remote(&engine, &identity, 2, "Forged", false).unwrap();
        forged.signed.sig = "00".repeat(64);
        assert!(engine
            .sealed_profiles_arrived(vec![forged], &asked)
            .is_empty());
        assert_eq!(engine.my_fields(&identity.id()).unwrap(), before);
        assert_eq!(engine.store.meta(META_OWN_SYNC).unwrap(), journal);
        assert!(!engine.net.profiles.own_refreshed);
        assert!(!engine.profiles_answer_complete(&asked));
        assert!(engine.take_need_profiles().contains(&identity.id()));
        assert!(engine.profile_to_upload().is_none());
    }

    #[test]
    fn missing_known_publication_does_not_complete_own_lookup() {
        let (mut engine, identity) = fixture();
        engine
            .update_my_profile("Local pending", "bio", PhotoChange::Keep)
            .unwrap();
        engine.profile_changed(&identity.id());
        let asked = engine.take_need_profiles();
        engine.sealed_profiles_arrived(Vec::new(), &asked);
        assert!(!engine.profiles_answer_complete(&asked));
        assert!(!engine.net.profiles.own_refreshed);
        assert!(engine.profile_to_upload().is_none());
        assert_eq!(engine.own_profile_journal().unwrap().sync.published, 1);
        assert_eq!(engine.own_profile_journal().unwrap().sync.pending, Some(2));
    }

    #[test]
    fn missing_profile_allows_explicit_bootstrap_but_not_clean_inherited_seed() {
        let (mut engine, identity) = fixture();
        engine
            .save_own_profile_journal(&OwnJournal {
                sync: OwnProfileSync::migrate(1, 0),
                upload: None,
            })
            .unwrap();
        engine.profiles_session_start(true);
        let asked = engine.take_need_profiles();
        engine.sealed_profiles_arrived(Vec::new(), &asked);
        assert!(engine.profiles_answer_complete(&asked));
        assert!(engine.profile_to_upload().is_some());
        engine.initialize_linked_profile_sync().unwrap();
        engine.profiles_session_start(true);
        let asked = engine.take_need_profiles();
        engine.sealed_profiles_arrived(Vec::new(), &asked);
        assert!(!engine.profiles_answer_complete(&asked));
        assert!(!engine.net.profiles.own_refreshed);
        assert!(engine.profile_to_upload().is_none());
        assert!(engine.take_need_profiles().contains(&identity.id()));
    }
    #[test]
    fn stale_signed_own_record_does_not_satisfy_publication_floor() {
        let (mut engine, identity) = fixture();
        engine
            .update_my_profile("Local pending", "bio", PhotoChange::Keep)
            .unwrap();
        engine.profile_changed(&identity.id());
        let asked = engine.take_need_profiles();
        let stale = remote(&engine, &identity, 0, "Old", false).unwrap();
        engine.sealed_profiles_arrived(vec![stale], &asked);
        assert!(!engine.profiles_answer_complete(&asked));
        assert!(!engine.net.profiles.own_refreshed);
        assert_eq!(engine.own_profile_journal().unwrap().sync.published, 1);
        assert_eq!(engine.own_profile_journal().unwrap().sync.pending, Some(2));
        assert!(engine.profile_to_upload().is_none());
    }
}
