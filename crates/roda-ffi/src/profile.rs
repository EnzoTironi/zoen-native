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

use roda_log::profile::{
    open_profile, open_share, seal_profile, seal_share, share_context, sign_agreement, sign_upload,
    verify_agreement, verify_upload, AgreementKey, DeviceSigned, ProfileFields, ProfileKey,
    ProfilePhoto, SealedShare, MAX_CIPHERTEXT,
};
use roda_log::profile_sync::{Observe, OwnProfileSync};
use roda_proto::{AgreementKeyRecord, SealedProfile};
use roda_types::*;
use sha2::{Digest, Sha256};

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
use roda_log::profile::MAX_PHOTO_BYTES;
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

impl Engine {
    fn own_profile_journal(&self) -> R<OwnJournal> {
        if let Some(encoded) = self.store.meta(META_OWN_SYNC)? {
            return serde_json::from_str(&encoded).map_err(|_| invalid("profile journal".into()));
        }
        let me = self.me_id()?;
        let (_, _, seen, _) = self.my_fields(&me)?;
        let uploaded = self
            .store
            .meta(META_UPLOADED)?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        Ok(OwnJournal {
            sync: OwnProfileSync::migrate(seen, uploaded),
            upload: None,
        })
    }

    fn save_own_profile_journal(&self, journal: &OwnJournal) -> R<()> {
        self.store.set_meta(
            META_OWN_SYNC,
            &serde_json::to_string(journal).map_err(|_| invalid("profile journal".into()))?,
        )?;
        Ok(())
    }

    fn next_profile_version(&self, seen: u64) -> R<u64> {
        self.own_profile_journal()?
            .sync
            .edit(seen)
            .ok_or_else(|| invalid("profile version".into()))
    }

    fn mark_profile_intent(&self, version: u64) -> R<()> {
        let mut journal = self.own_profile_journal()?;
        journal.sync.pending = Some(version);
        journal.upload = None;
        self.save_own_profile_journal(&journal)
    }

    pub(crate) fn initialize_linked_profile_sync(&self) -> R<()> {
        self.save_own_profile_journal(&OwnJournal {
            sync: OwnProfileSync::default(),
            upload: None,
        })
    }
    fn my_fields(&self, me: &str) -> R<(ProfileKey, u64, u64, ProfileFields)> {
        let row = self
            .store
            .profile_key(me)?
            .ok_or_else(|| CoreError::NotFound {
                what: "profile".into(),
            })?;
        let key = ProfileKey::from_hex(&row.key).ok_or_else(|| invalid("profile key".into()))?;
        let fields = row
            .fields
            .as_deref()
            .and_then(ProfileFields::from_bytes)
            .unwrap_or_default();
        Ok((key, row.key_version, row.seen_version, fields))
    }

    pub(crate) fn fields_of(&self, id: &str) -> Option<(u64, ProfileFields)> {
        let row = self.store.profile_key(id).ok()??;
        let fields = ProfileFields::from_bytes(row.fields.as_deref()?)?;
        Some((row.seen_version, fields))
    }

    /// A new account's profile key, first version and agreement key. Returns the agreement
    /// secret for the vault.
    pub(crate) fn init_profile(&mut self, me: &str, name: &str) -> R<[u8; 32]> {
        self.init_profile_with_agreement(me, name, AgreementKey::generate())
    }

    pub(crate) fn init_profile_with_agreement(
        &mut self,
        me: &str,
        name: &str,
        agreement: AgreementKey,
    ) -> R<[u8; 32]> {
        let key = ProfileKey::generate();
        self.store.put_profile_key(me, &key.to_hex(), 1)?;
        let fields = ProfileFields {
            name: name.to_string(),
            ..Default::default()
        };
        self.store.set_profile_fields(me, 1, &fields.to_bytes())?;
        self.mark_profile_intent(1)?;
        let secret = agreement.secret();
        self.net.profiles.agreement = Some(agreement);
        self.net.profiles.shares_dirty = true;
        self.net
            .mls
            .wake_recovery_many(self.net.synced.iter().cloned());
        Ok(secret)
    }

    /// Loads the agreement key at unlock. An account from before profiles gets its profile
    /// key here; `Some(secret)` means a new agreement key to keep in the vault.
    pub(crate) fn unlock_profile(&mut self, secret: Option<Vec<u8>>) -> R<Option<[u8; 32]>> {
        let me = self.me_id()?;
        if self.store.profile_key(&me)?.is_none() {
            let name = self.my_profile().map(|p| p.name).unwrap_or_default();
            let key = ProfileKey::generate();
            self.store.put_profile_key(&me, &key.to_hex(), 1)?;
            let fields = ProfileFields {
                name,
                ..Default::default()
            };
            self.store.set_profile_fields(&me, 1, &fields.to_bytes())?;
            self.mark_profile_intent(1)?;
        }
        self.net.profiles.shares_dirty = true;
        self.net
            .mls
            .wake_recovery_many(self.net.synced.iter().cloned());
        match secret.and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok()) {
            Some(s) => {
                self.net.profiles.agreement = Some(AgreementKey::from_secret(s));
                Ok(None)
            }
            None => {
                let a = AgreementKey::generate();
                let s = a.secret();
                self.net.profiles.agreement = Some(a);
                Ok(Some(s))
            }
        }
    }

    // ── editing ──

    pub fn update_my_profile(&mut self, name: &str, bio: &str, photo: PhotoChange) -> R<()> {
        let me = self.me_id()?;
        let (_, _, version, mut fields) = self.my_fields(&me)?;
        let name = name.trim();
        let bio = bio.trim();
        if name.is_empty() {
            return Err(invalid(t("Diga seu nome.", "Tell us your name.")));
        }
        if name.chars().count() > MAX_NAME {
            return Err(invalid(t("Nome longo demais.", "That name is too long.")));
        }
        if bio.chars().count() > MAX_BIO {
            return Err(invalid(t(
                "A bio tem no máximo 280 caracteres.",
                "Your bio fits 280 characters.",
            )));
        }
        fields.name = name.to_string();
        fields.bio = bio.to_string();
        let next = self.next_profile_version(version)?;
        self.store.raw_execute("SAVEPOINT profile_edit")?;
        let edited = (|| -> R<()> {
            match photo {
                PhotoChange::Keep => {}
                PhotoChange::Remove => fields.photo = None,
                PhotoChange::Set { bytes, mime } => {
                    fields.photo = Some(self.profile_photo(&bytes, &mime)?)
                }
            }
            self.store
                .set_profile_fields(&me, next, &fields.to_bytes())?;
            self.mark_profile_intent(next)?;
            if let Some(mut ident) = self.identities.get(&me).cloned() {
                ident.name = fields.name.clone();
                ident.bio = fields.bio.clone();
                self.store.put_identity(&ident, None)?;
            }
            Ok(())
        })();
        if let Err(error) = edited {
            self.store.raw_execute("ROLLBACK TO profile_edit")?;
            self.store.raw_execute("RELEASE profile_edit")?;
            return Err(error);
        }
        self.store.raw_execute("RELEASE profile_edit")?;
        if let Some(ident) = self.identities.get_mut(&me) {
            ident.name = fields.name;
            ident.bio = fields.bio;
        }
        self.index_dirty = true;
        self.net.wake();
        Ok(())
    }

    /// The name and bio half of `update_profile` (which also changes the @).
    pub(crate) fn set_profile_text(&mut self, name: &str, bio: &str) -> R<()> {
        let me = self.me_id()?;
        let Ok((_, _, version, mut fields)) = self.my_fields(&me) else {
            return Ok(());
        };
        if fields.name != name || fields.bio != bio {
            fields.name = name.to_string();
            fields.bio = bio.to_string();
            self.write_my_fields(&me, self.next_profile_version(version)?, &fields)?;
            self.net.wake();
        }
        Ok(())
    }

    fn profile_photo(&mut self, bytes: &[u8], mime: &str) -> R<ProfilePhoto> {
        if !matches!(
            mime,
            "image/jpeg" | "image/png" | "image/heic" | "image/webp"
        ) {
            return Err(invalid(t(
                "Use uma foto JPEG, PNG, HEIC ou WebP.",
                "Use a JPEG, PNG, HEIC or WebP photo.",
            )));
        }
        if bytes.is_empty() || bytes.len() > MAX_PHOTO_BYTES {
            return Err(invalid(t(
                "A foto precisa ter até 5 MB.",
                "The photo must be 5 MB or less.",
            )));
        }
        let sha = hex::encode(Sha256::digest(bytes));
        self.store
            .cache_imported_media(&sha, mime, bytes, now_ms())?;
        let mut m = MediaRef {
            sha256: sha.clone(),
            mime: mime.to_string(),
            width: 0,
            height: 0,
            bytes: bytes.len() as u64,
            key: None,
            blob: None,
        };
        self.seal_for_relay(&mut m)?;
        Ok(ProfilePhoto {
            sha256: sha,
            blob: m.blob.unwrap_or_default(),
            key: m.key.unwrap_or_default(),
            mime: mime.to_string(),
        })
    }

    fn write_my_fields(&mut self, me: &str, version: u64, fields: &ProfileFields) -> R<()> {
        self.store.raw_execute("SAVEPOINT profile_fields")?;
        let changed = (|| -> R<()> {
            self.store
                .set_profile_fields(me, version, &fields.to_bytes())?;
            self.mark_profile_intent(version)?;
            Ok(())
        })();
        if let Err(error) = changed {
            self.store.raw_execute("ROLLBACK TO profile_fields")?;
            self.store.raw_execute("RELEASE profile_fields")?;
            return Err(error);
        }
        self.store.raw_execute("RELEASE profile_fields")?;
        if let Some(mut ident) = self.identities.get(me).cloned() {
            ident.name = fields.name.clone();
            ident.bio = fields.bio.clone();
            self.store.put_identity(&ident, None)?;
            self.identities.insert(me.to_string(), ident);
            self.index_dirty = true;
        }
        Ok(())
    }

    /// A fresh key from the next version on. Whoever had the old one keeps what it opened
    /// and nothing newer; everyone not blocked gets the new key in the next share pass.
    fn rotate_profile_key(&mut self) -> R<()> {
        let me = self.me_id()?;
        let (_, _, version, fields) = self.my_fields(&me)?;
        let next = self.next_profile_version(version)?;
        self.store
            .put_profile_key(&me, &ProfileKey::generate().to_hex(), next)?;
        self.write_my_fields(&me, next, &fields)?;
        self.net.profiles.shares_dirty = true;
        self.net.wake();
        Ok(())
    }

    pub fn block_person(&mut self, id: &str) -> R<()> {
        let me = self.me_id()?;
        if id == me || id.len() != 64 {
            return Err(invalid(t(
                "Não dá para bloquear essa pessoa.",
                "You can't block that person.",
            )));
        }
        self.store.raw_execute("SAVEPOINT profile_block")?;
        let changed = (|| -> R<()> {
            if self.store.block(id, now_ms())? {
                self.rotate_profile_key()?;
            }
            Ok(())
        })();
        if let Err(error) = changed {
            self.store.raw_execute("ROLLBACK TO profile_block")?;
            self.store.raw_execute("RELEASE profile_block")?;
            return Err(error);
        }
        self.store.raw_execute("RELEASE profile_block")?;
        Ok(())
    }

    pub fn unblock_person(&mut self, id: &str) -> R<()> {
        if self.store.unblock(id)? {
            self.net.profiles.shares_dirty = true;
            self.net.wake();
        }
        Ok(())
    }

    pub fn blocked_people(&self) -> R<Vec<IdentityId>> {
        Ok(self.store.blocked()?)
    }

    // ── reading ──

    pub fn profile_view(&self, id: &str) -> R<ProfileDto> {
        let ident = self.identities.get(id);
        let held = self.fields_of(id);
        if ident.is_none() && held.is_none() {
            return Err(CoreError::NotFound {
                what: "profile".into(),
            });
        }
        let is_me = self.me.as_deref() == Some(id);
        let agent = ident.is_some_and(|i| i.kind == IdentityKind::Agent);
        let (name, bio, version, photo) = match (&held, agent) {
            (Some((v, f)), false) => (
                Some(f.name.clone()),
                Some(f.bio.clone()),
                *v,
                f.photo.clone(),
            ),
            (_, true) => {
                let i = ident.expect("agent is known");
                (Some(i.name.clone()), Some(i.bio.clone()), 0, None)
            }
            (None, false) => (None, None, 0, None),
        };
        let photo_ready = photo
            .as_ref()
            .is_some_and(|p| p.valid() && matches!(self.store.media_bytes_len(&p.sha256), Ok(Some(size)) if size > 0 && size <= MAX_PHOTO_BYTES as u64));
        Ok(ProfileDto {
            identity_id: id.to_string(),
            handle: ident.map(|i| i.handle.clone()).unwrap_or_default(),
            name,
            bio,
            photo_mime: photo.as_ref().map(|p| p.mime.clone()),
            photo_sha256: photo.map(|p| p.sha256),
            photo_ready,
            version,
            tint_hex: ident.map(|i| i.tint_hex.clone()).unwrap_or_default(),
            is_me,
            blocked: self.store.blocked()?.iter().any(|b| b == id),
        })
    }

    pub(crate) fn profile_photo_bytes(
        &self,
        id: &str,
        expected: &str,
    ) -> R<Option<ProfilePhotoBytes>> {
        let Some((_, fields)) = self.fields_of(id) else {
            return Ok(None);
        };
        let Some(photo) = fields.photo.filter(|p| p.sha256 == expected && p.valid()) else {
            return Ok(None);
        };
        let Some(bytes) = self.store.media_bytes_bounded(expected, MAX_PHOTO_BYTES)? else {
            return Ok(None);
        };
        if bytes.is_empty()
            || bytes.len() > MAX_PHOTO_BYTES
            || hex::encode(Sha256::digest(&bytes)) != expected
        {
            return Ok(None);
        }
        Ok(Some(ProfilePhotoBytes {
            identity_id: id.into(),
            sha256: expected.into(),
            mime: photo.mime,
            bytes,
        }))
    }

    /// What the directory says about a person, with the name and bio this device can
    /// decrypt (or the @handle when it can't).
    pub(crate) fn overlay_profile(&self, p: &mut Identity) {
        if p.kind != IdentityKind::Person {
            return;
        }
        match self.fields_of(&p.id) {
            Some((_, f)) if !f.name.is_empty() => {
                p.name = f.name;
                p.bio = f.bio;
            }
            _ => {
                p.name = format!("@{}", p.handle);
                p.bio = String::new();
            }
        }
    }

    fn apply_fields(&mut self, id: &str) -> R<()> {
        let Some(mut ident) = self.identities.get(id).cloned() else {
            self.net.unknown.insert(id.to_string());
            return Ok(());
        };
        self.overlay_profile(&mut ident);
        self.store.put_identity(&ident, None)?;
        self.identities.insert(id.to_string(), ident);
        self.index_dirty = true;
        Ok(())
    }

    /// Profile photos this device holds a key for but not the bytes.
    pub(crate) fn wanted_profile_photos(&self) -> Vec<Wanted> {
        self.store
            .profile_keys()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|row| {
                let photo = ProfileFields::from_bytes(row.fields.as_deref()?)?.photo?;
                if matches!(self.store.media(&photo.sha256), Ok(Some(_))) {
                    return None;
                }
                Some(Wanted {
                    plain: photo.sha256,
                    blob: photo.blob,
                    key: photo.key,
                    mime: photo.mime,
                    space: String::new(),
                    profile: Some(row.identity),
                })
            })
            .collect()
    }

    // ── shares ──

    /// Reacts to a sequenced or local event: membership changes may need shares, a share
    /// addressed to me brings a key.
    pub(crate) fn note_profile_event(&mut self, e: &Event) {
        match &e.body {
            EventBody::SpaceCreated { .. } | EventBody::MemberAdded { .. } => {
                self.net.profiles.shares_dirty = true;
            }
            EventBody::ProfileKeyShared { shares, .. } => {
                let Some(me) = self.me.clone() else { return };
                if e.author == me {
                    return;
                }
                let Some(agreement) = &self.net.profiles.agreement else {
                    return;
                };
                let Some(share) = shares.iter().find(|s| s.to == me) else {
                    return;
                };
                let sealed = SealedShare {
                    ephemeral: share.ephemeral.clone(),
                    sealed: share.sealed.clone(),
                };
                let Some((key, version)) =
                    open_share(agreement, &sealed, &share_context(&e.space, &e.author, &me))
                else {
                    return;
                };
                if matches!(
                    self.store
                        .put_profile_key(&e.author, &key.to_hex(), version),
                    Ok(true)
                ) {
                    self.net.profiles.need_profiles.insert(e.author.clone());
                }
            }
            _ => {}
        }
    }

    fn already_shared(&mut self, me: &str, version: u64) -> HashSet<IdentityId> {
        if let Some((v, set)) = &self.net.profiles.shared {
            if *v == version {
                return set.clone();
            }
        }
        let mut set = HashSet::new();
        let mut take = |e: &Event| {
            if let EventBody::ProfileKeyShared { version: v, shares } = &e.body {
                if e.author == me && *v == version {
                    set.extend(shares.iter().map(|s| s.to.clone()));
                }
            }
        };
        for log in self.net.synced.iter().filter_map(|s| self.logs.get(s)) {
            log.events().iter().for_each(&mut take);
        }
        for p in self.store.outbox().unwrap_or_default() {
            if !p.failed {
                take(&p.event);
            }
        }
        self.net.profiles.shared = Some((version, set.clone()));
        set
    }

    /// Shares my key with everyone I share a relay Space with who doesn't hold it yet.
    /// People whose agreement key isn't known are queued for a lookup (which re-runs this).
    pub fn profile_share_pass(&mut self) -> R<bool> {
        if !self.net.profiles.supported || !std::mem::take(&mut self.net.profiles.shares_dirty) {
            return Ok(false);
        }
        let (Some(me), Some(author)) = (self.me.clone(), self.net.author.clone()) else {
            return Ok(false);
        };
        let Ok((key, key_version, _, _)) = self.my_fields(&me) else {
            return Ok(false);
        };
        let blocked: HashSet<IdentityId> = self.store.blocked()?.into_iter().collect();
        let mut done = self.already_shared(&me, key_version);
        let mut spaces: Vec<SpaceId> = self
            .net
            .synced
            .iter()
            .filter(|s| self.state.spaces.contains_key(*s))
            .cloned()
            .collect();
        spaces.sort();
        let mut wrote = false;
        for space in spaces {
            let members: Vec<IdentityId> = self.state.spaces[&space]
                .members
                .iter()
                .map(|(m, _)| m.clone())
                .collect();
            if !members.contains(&me) {
                continue;
            }
            let mut shares = Vec::new();
            for m in members {
                if m == me
                    || done.contains(&m)
                    || blocked.contains(&m)
                    || self.net.profiles.no_agreement.contains(&m)
                    || self
                        .identities
                        .get(&m)
                        .is_some_and(|i| i.kind == IdentityKind::Agent)
                {
                    continue;
                }
                let Some(public) = self.store.peer_agreement_key(&m)? else {
                    self.net.profiles.need_agreement.insert(m);
                    continue;
                };
                if let Some(s) =
                    seal_share(&key, key_version, &public, &share_context(&space, &me, &m))
                {
                    done.insert(m.clone());
                    shares.push(ProfileKeyShare {
                        to: m,
                        ephemeral: s.ephemeral,
                        sealed: s.sealed,
                    });
                }
            }
            for chunk in shares.chunks(SHARES_PER_EVENT) {
                self.append_synced(
                    &space,
                    &author,
                    now_ms(),
                    EventBody::ProfileKeyShared {
                        version: key_version,
                        shares: chunk.to_vec(),
                    },
                )?;
                wrote = true;
            }
        }
        self.net.profiles.shared = Some((key_version, done));
        Ok(wrote)
    }

    // ── relay traffic ──

    /// A new session: refresh every profile I hold a key for and retry missing shares.
    pub fn profiles_session_start(&mut self, supported: bool) {
        self.net
            .mls
            .wake_recovery_many(self.net.synced.iter().cloned());
        let p = &mut self.net.profiles;
        p.supported = supported;
        p.no_agreement.clear();
        p.shares_dirty = true;
        p.own_refreshed = false;
        p.fetching.clear();
        p.refetch.clear();
        p.need_profiles = self
            .store
            .profile_keys()
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.identity)
            .collect();
    }

    pub fn agreement_to_publish(&self) -> Option<(String, DeviceSigned)> {
        if !self.net.profiles.supported {
            return None;
        }
        let public = self.net.profiles.agreement.as_ref()?.public_hex();
        if self.store.meta(META_AGREEMENT).ok().flatten().as_deref() == Some(public.as_str()) {
            return None;
        }
        let signed = sign_agreement(self.net.author.as_ref()?, &public)?;
        Some((public, signed))
    }

    pub fn agreement_published(&mut self, public: &str) {
        let _ = self.store.set_meta(META_AGREEMENT, public);
    }

    fn upload_due(&self) -> bool {
        self.net.profiles.supported
            && self.net.profiles.own_refreshed
            && self.own_profile_journal().is_ok_and(|journal| {
                journal.sync.pending.is_some() && !journal.sync.key_unavailable()
            })
    }

    /// My sealed profile when the relay doesn't have the current version yet.
    pub fn profile_to_upload(&mut self) -> Option<SealedProfile> {
        if !self.upload_due() {
            return None;
        }
        let me = self.me.clone()?;
        let (key, _, version, fields) = self.my_fields(&me).ok()?;
        // Publishing the reference before its encrypted blob would strand contacts.
        if fields
            .photo
            .as_ref()
            .is_some_and(|photo| self.store.upload_pending(&photo.blob).unwrap_or(true))
        {
            return None;
        }
        let mut journal = self.own_profile_journal().ok()?;
        if journal.sync.pending != Some(version) {
            return None;
        }
        if let Some(upload) = &journal.upload {
            if upload.version == version
                && self.net.author.as_ref().is_some_and(|author| {
                    author.device.as_deref() == Some(upload.device.as_str())
                        && author.cert.as_deref() == Some(upload.cert.as_str())
                })
            {
                return Some(upload.sealed(&me));
            }
        }
        let ciphertext = seal_profile(&key, &me, version, &fields);
        if ciphertext.len() > MAX_CIPHERTEXT {
            return None;
        }
        let signed = sign_upload(self.net.author.as_ref()?, version, &ciphertext)?;
        let s = SealedProfile {
            identity: me,
            version,
            ciphertext,
            signed,
        };
        journal.upload = Some(StoredUpload {
            version,
            ciphertext: s.ciphertext.clone(),
            device: s.signed.device.clone(),
            cert: s.signed.cert.clone(),
            sig: s.signed.sig.clone(),
        });
        self.save_own_profile_journal(&journal).ok()?;
        Some(s)
    }

    /// The relay answered an upload. A stale version means another of my devices got
    /// further: move past it.
    pub fn profile_upload_answered(&mut self, version: u64, result: Result<(), String>) {
        let Ok(mut journal) = self.own_profile_journal() else {
            return;
        };
        match result {
            Ok(()) => {
                journal.sync.acknowledge(version);
                if journal
                    .upload
                    .as_ref()
                    .is_some_and(|upload| upload.version == version)
                {
                    journal.upload = None;
                }
                let _ = self.save_own_profile_journal(&journal);
            }
            Err(error) if error.contains("stale profile version") => {
                // Fetch the signed relay floor before rebasing. Never blindly increment
                // an old-key payload or mistake a lost acknowledgement for an edit.
                if let Some(me) = self.me.clone() {
                    self.net.profiles.need_profiles.insert(me);
                    self.net.profiles.own_refreshed = false;
                }
            }
            Err(error) => crate::net::tracing_like(&format!("profile upload refused: {error}")),
        }
    }

    pub fn take_need_agreement(&self) -> Vec<IdentityId> {
        if !self.net.profiles.supported {
            return Vec::new();
        }
        self.net
            .profiles
            .need_agreement
            .iter()
            .take(BATCH)
            .cloned()
            .collect()
    }

    pub fn agreement_keys_arrived(
        &mut self,
        records: Vec<AgreementKeyRecord>,
        asked: &[IdentityId],
    ) {
        let mut found = HashSet::new();
        for r in records {
            if asked.contains(&r.identity) && verify_agreement(&r.identity, &r.public, &r.signed) {
                let _ = self.store.put_peer_agreement_key(&r.identity, &r.public);
                found.insert(r.identity);
            }
        }
        let p = &mut self.net.profiles;
        for id in asked {
            p.need_agreement.remove(id);
            if !found.contains(id) {
                p.no_agreement.insert(id.clone());
            }
        }
        p.shares_dirty = true;
        self.net.mls.wake_recovery_many(
            self.net
                .synced
                .iter()
                .filter(|space| {
                    self.mls_roster(space)
                        .is_some_and(|roster| asked.iter().any(|id| roster.contains_key(id)))
                })
                .cloned(),
        );
    }

    pub fn take_need_profiles(&mut self) -> Vec<IdentityId> {
        if !self.net.profiles.supported {
            return Vec::new();
        }
        let asked = self
            .net
            .profiles
            .need_profiles
            .iter()
            .take(BATCH)
            .cloned()
            .collect::<Vec<_>>();
        for id in &asked {
            self.net.profiles.fetching.insert(id.clone());
            self.net.profiles.refetch.remove(id);
        }
        asked
    }

    /// Opens what the relay served. Returns whose profile changed on this device.
    pub fn sealed_profiles_arrived(
        &mut self,
        list: Vec<SealedProfile>,
        asked: &[IdentityId],
    ) -> Vec<IdentityId> {
        for id in asked {
            self.net.profiles.fetching.remove(id);
            if !self.net.profiles.refetch.remove(id) {
                self.net.profiles.need_profiles.remove(id);
            }
        }
        let mut changed = Vec::new();
        let me = self.me.clone();
        let own_asked = me.as_ref().is_some_and(|me| asked.contains(me));
        let own_present = me
            .as_ref()
            .is_some_and(|me| list.iter().any(|record| &record.identity == me));
        let own_sync = own_asked
            .then(|| self.own_profile_journal().ok().map(|journal| journal.sync))
            .flatten();
        let mut own_verified = false;
        if own_asked {
            self.net.profiles.own_refreshed = false;
        }
        for p in list {
            if !asked.contains(&p.identity)
                || p.version == 0
                || p.ciphertext.len() > MAX_CIPHERTEXT
                || !verify_upload(&p.identity, p.version, &p.ciphertext, &p.signed)
            {
                continue;
            }
            let Ok(Some(row)) = self.store.profile_key(&p.identity) else {
                continue;
            };
            if self.me.as_deref() == Some(p.identity.as_str()) {
                own_verified |= own_sync
                    .as_ref()
                    .is_some_and(|sync| p.version >= sync.published);
                if self.open_own_profile(&p).unwrap_or(false) {
                    changed.push(p.identity);
                }
                continue;
            }
            if p.version <= row.seen_version || p.version < row.key_version {
                continue;
            }
            let Some(key) = ProfileKey::from_hex(&row.key) else {
                continue;
            };
            let Some(fields) = open_profile(&key, &p.identity, p.version, &p.ciphertext) else {
                continue;
            };
            if self
                .store
                .set_profile_fields(&p.identity, p.version, &fields.to_bytes())
                .is_ok()
                && self.apply_fields(&p.identity).is_ok()
            {
                changed.push(p.identity);
            }
        }
        if own_asked {
            let me = me.expect("own profile was requested");
            let bootstrap_absent = !own_present
                && own_sync.as_ref().is_some_and(|sync| {
                    sync.published == 0 && sync.pending.is_some() && !sync.key_unavailable()
                });
            let complete = (own_verified || bootstrap_absent)
                && !self.net.profiles.need_profiles.contains(&me);
            self.net.profiles.own_refreshed = complete;
            if !complete {
                self.net.profiles.need_profiles.insert(me);
            }
        }
        changed
    }

    /// Invalid/stale own records and a missing known publication remain work.
    /// The network layer uses this to apply its ordinary bounded retry deadline.
    pub fn profiles_answer_complete(&self, asked: &[IdentityId]) -> bool {
        !asked
            .iter()
            .any(|id| self.net.profiles.need_profiles.contains(id))
    }

    fn open_own_profile(&mut self, profile: &SealedProfile) -> R<bool> {
        let me = self.me_id()?;
        let (key, key_version, local_version, local_fields) = self.my_fields(&me)?;
        let mut journal = self.own_profile_journal()?;
        if profile.version < journal.sync.published || profile.version < key_version {
            return Ok(false);
        }
        let Some(fields) = open_profile(&key, &me, profile.version, &profile.ciphertext) else {
            journal.sync.cannot_open(profile.version);
            self.save_own_profile_journal(&journal)?;
            return Ok(false);
        };
        let exact = journal
            .upload
            .as_ref()
            .is_some_and(|upload| upload.sealed(&me) == *profile);
        let outcome = journal
            .sync
            .observe_opened(profile.version, exact)
            .ok_or_else(|| invalid("profile version".into()))?;
        let (version, adopted) = match outcome {
            Observe::Adopt => (profile.version, Some(fields)),
            Observe::Preserve { version } => (version.max(local_version), Some(local_fields)),
            Observe::Acknowledged | Observe::Ignore => (local_version, None),
        };
        // Old in-flight ACKs are version-scoped; only the current pending intent
        // may clear itself. Invalidate a ciphertext only when its version moved.
        if journal
            .upload
            .as_ref()
            .is_some_and(|upload| Some(upload.version) != journal.sync.pending)
        {
            journal.upload = None;
        }
        self.store.raw_execute("SAVEPOINT own_profile_receive")?;
        let applied = (|| -> R<()> {
            if let Some(fields) = &adopted {
                self.store
                    .set_profile_fields(&me, version, &fields.to_bytes())?;
            }
            self.save_own_profile_journal(&journal)?;
            Ok(())
        })();
        if let Err(error) = applied {
            self.store.raw_execute("ROLLBACK TO own_profile_receive")?;
            self.store.raw_execute("RELEASE own_profile_receive")?;
            return Err(error);
        }
        self.store.raw_execute("RELEASE own_profile_receive")?;
        if adopted.is_some() {
            self.apply_fields(&me)?;
        }
        Ok(matches!(outcome, Observe::Adopt))
    }

    /// The relay says someone's profile moved on.
    pub fn profile_changed(&mut self, id: &str) {
        if matches!(self.store.profile_key(id), Ok(Some(_))) {
            if self.net.profiles.fetching.contains(id) {
                self.net.profiles.refetch.insert(id.to_string());
            }
            self.net.profiles.need_profiles.insert(id.to_string());
            if self.me.as_deref() == Some(id) {
                self.net.profiles.own_refreshed = false;
            }
        }
    }

    /// Nothing about profiles is waiting on the relay.
    pub fn profiles_settled(&self) -> bool {
        let p = &self.net.profiles;
        if !p.supported {
            return true;
        }
        p.need_agreement.is_empty()
            && p.need_profiles.is_empty()
            && !p.shares_dirty
            && self.agreement_to_publish().is_none()
            && !self.upload_due()
            && self.own_profile_journal().is_ok_and(|journal| {
                journal.sync.pending.is_none() && !journal.sync.key_unavailable()
            })
    }
}

#[cfg(test)]
mod own_sync_tests {
    use super::*;
    use roda_log::{Author, Signer};

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
    ) -> SealedProfile {
        let author = Author::device(identity, Signer::from_secret(&[92; 32]));
        let key = if rotated {
            ProfileKey::generate()
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
        );
        SealedProfile {
            identity: identity.id(),
            version,
            signed: sign_upload(&author, version, &ciphertext).unwrap(),
            ciphertext,
        }
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
        let mut incoming = remote(&engine, &identity, 2, "Other device", false);
        incoming.ciphertext = seal_profile(
            &engine.my_fields(&identity.id()).unwrap().0,
            &identity.id(),
            2,
            &fields,
        );
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
    fn concurrent_self_profile_preserves_pending_and_exact_retry_across_network_reset() {
        let (mut engine, identity) = fixture();
        engine
            .update_my_profile("Local pending", "bio", PhotoChange::Keep)
            .unwrap();
        let old = engine.profile_to_upload().unwrap();
        let incoming = remote(&engine, &identity, 4, "Other device", false);
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
            roda_types::new_ulid(now_ms())
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
        let baseline = remote(&reopened, &identity, 1, "Original", false);
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
        let incoming = remote(&engine, &identity, 8, "Rotated", true);
        engine.sealed_profiles_arrived(vec![incoming], &[identity.id()]);
        assert!(engine.own_profile_journal().unwrap().sync.key_unavailable());
        let old = remote(&engine, &identity, 2, "Old key", false);
        engine.sealed_profiles_arrived(vec![old], &[identity.id()]);
        engine.profile_upload_answered(2, Ok(()));
        engine.profile_upload_answered(100, Ok(()));
        let newer_old_key = remote(&engine, &identity, 101, "Old key published higher", false);
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
        let mut forged = remote(&engine, &identity, 2, "Forged", false);
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
        let stale = remote(&engine, &identity, 0, "Old", false);
        engine.sealed_profiles_arrived(vec![stale], &asked);
        assert!(!engine.profiles_answer_complete(&asked));
        assert!(!engine.net.profiles.own_refreshed);
        assert_eq!(engine.own_profile_journal().unwrap().sync.published, 1);
        assert_eq!(engine.own_profile_journal().unwrap().sync.pending, Some(2));
        assert!(engine.profile_to_upload().is_none());
    }
}
