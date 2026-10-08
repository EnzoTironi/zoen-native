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
use roda_proto::{AgreementKeyRecord, SealedProfile};
use roda_types::*;
use sha2::{Digest, Sha256};

use crate::engine::{now_ms, Engine, R};
use crate::i18n::t;
use crate::media::Wanted;
use crate::CoreError;

pub(crate) const VAULT_AGREEMENT: &str = "zoen.agreement.v1";
const META_UPLOADED: &str = "profile:uploaded";
const META_AGREEMENT: &str = "agreement:published";
/// Shares per event until MLS carries them (each is ~200 bytes of hex).
const SHARES_PER_EVENT: usize = 200;
const MAX_NAME: usize = 64;
const MAX_BIO: usize = 280;
const MAX_PHOTO_BYTES: usize = 5 * 1024 * 1024;
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
    /// The upload in flight, so a retry resends identical bytes.
    sealed: Option<SealedProfile>,
}

impl ProfileNet {
    pub(crate) fn forget_shared(&mut self) {
        self.shared = None;
    }
}

/// How `update_my_profile` treats the photo.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum PhotoChange {
    Keep,
    Remove,
    Set { bytes: Vec<u8>, mime: String },
}

/// A person's profile as this device can see it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ProfileDto {
    pub identity_id: String,
    pub handle: String,
    /// `None` when this device holds no key for them (not a contact): show `@handle`.
    pub name: Option<String>,
    pub bio: Option<String>,
    /// The photo's sha256; its bytes come from `media(sha256)` once `photo_ready`.
    pub photo_sha256: Option<String>,
    pub photo_ready: bool,
    /// The profile version shown (0 = none readable yet).
    pub version: u64,
    pub tint_hex: String,
    pub is_me: bool,
    pub blocked: bool,
}

fn invalid(reason: String) -> CoreError {
    CoreError::Invalid { reason }
}

impl Engine {
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

    fn fields_of(&self, id: &str) -> Option<(u64, ProfileFields)> {
        let row = self.store.profile_key(id).ok()??;
        let fields = ProfileFields::from_bytes(row.fields.as_deref()?)?;
        Some((row.seen_version, fields))
    }

    /// A new account's profile key, first version and agreement key. Returns the agreement
    /// secret for the vault.
    pub(crate) fn init_profile(&mut self, me: &str, name: &str) -> R<[u8; 32]> {
        let key = ProfileKey::generate();
        self.store.put_profile_key(me, &key.to_hex(), 1)?;
        let fields = ProfileFields {
            name: name.to_string(),
            ..Default::default()
        };
        self.store.set_profile_fields(me, 1, &fields.to_bytes())?;
        let agreement = AgreementKey::generate();
        let secret = agreement.secret();
        self.net.profiles.agreement = Some(agreement);
        self.net.profiles.shares_dirty = true;
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
        }
        self.net.profiles.shares_dirty = true;
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
        match photo {
            PhotoChange::Keep => {}
            PhotoChange::Remove => fields.photo = None,
            PhotoChange::Set { bytes, mime } => {
                fields.photo = Some(self.profile_photo(&bytes, &mime)?)
            }
        }
        self.write_my_fields(&me, version + 1, &fields)?;
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
            self.write_my_fields(&me, version + 1, &fields)?;
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
        self.import_media(&sha, bytes, mime)?;
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
        self.store
            .set_profile_fields(me, version, &fields.to_bytes())?;
        self.net.profiles.sealed = None;
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
        let next = version + 1;
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
        if self.store.block(id, now_ms())? {
            self.rotate_profile_key()?;
        }
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
            .is_some_and(|p| matches!(self.store.media(&p.sha256), Ok(Some(_))));
        Ok(ProfileDto {
            identity_id: id.to_string(),
            handle: ident.map(|i| i.handle.clone()).unwrap_or_default(),
            name,
            bio,
            photo_sha256: photo.map(|p| p.sha256),
            photo_ready,
            version,
            tint_hex: ident.map(|i| i.tint_hex.clone()).unwrap_or_default(),
            is_me,
            blocked: self.store.blocked()?.iter().any(|b| b == id),
        })
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
        let me = self.me.clone().unwrap_or_default();
        let p = &mut self.net.profiles;
        p.supported = supported;
        p.no_agreement.clear();
        p.shares_dirty = true;
        p.sealed = None;
        p.need_profiles = self
            .store
            .profile_keys()
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.identity)
            .filter(|id| *id != me)
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

    fn uploaded_version(&self) -> u64 {
        self.store
            .meta(META_UPLOADED)
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    fn upload_due(&self) -> bool {
        if !self.net.profiles.supported {
            return false;
        }
        let Some(me) = self.me.as_deref() else {
            return false;
        };
        matches!(self.store.profile_key(me), Ok(Some(r)) if r.seen_version > self.uploaded_version())
    }

    /// My sealed profile when the relay doesn't have the current version yet.
    pub fn profile_to_upload(&mut self) -> Option<SealedProfile> {
        if !self.upload_due() {
            return None;
        }
        let me = self.me.clone()?;
        let (key, _, version, fields) = self.my_fields(&me).ok()?;
        if let Some(s) = &self.net.profiles.sealed {
            if s.version == version {
                return Some(s.clone());
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
        self.net.profiles.sealed = Some(s.clone());
        Some(s)
    }

    /// The relay answered an upload. A stale version means another of my devices got
    /// further: move past it.
    pub fn profile_upload_answered(&mut self, version: u64, result: Result<(), String>) {
        self.net.profiles.sealed = None;
        match result {
            Ok(()) => {
                let _ = self.store.set_meta(META_UPLOADED, &version.to_string());
            }
            Err(e) if e.contains("stale profile version") => {
                if let Ok(me) = self.me_id() {
                    if let Ok((_, _, v, fields)) = self.my_fields(&me) {
                        let _ = self.write_my_fields(&me, v + 1, &fields);
                    }
                }
            }
            Err(e) => crate::net::tracing_like(&format!("profile upload refused: {e}")),
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
    }

    pub fn take_need_profiles(&self) -> Vec<IdentityId> {
        if !self.net.profiles.supported {
            return Vec::new();
        }
        self.net
            .profiles
            .need_profiles
            .iter()
            .take(BATCH)
            .cloned()
            .collect()
    }

    /// Opens what the relay served. Returns whose profile changed on this device.
    pub fn sealed_profiles_arrived(
        &mut self,
        list: Vec<SealedProfile>,
        asked: &[IdentityId],
    ) -> Vec<IdentityId> {
        for id in asked {
            self.net.profiles.need_profiles.remove(id);
        }
        let mut changed = Vec::new();
        for p in list {
            if !asked.contains(&p.identity)
                || !verify_upload(&p.identity, p.version, &p.ciphertext, &p.signed)
            {
                continue;
            }
            let Ok(Some(row)) = self.store.profile_key(&p.identity) else {
                continue;
            };
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
        changed
    }

    /// The relay says someone's profile moved on.
    pub fn profile_changed(&mut self, id: &str) {
        if self.me.as_deref() != Some(id) && matches!(self.store.profile_key(id), Ok(Some(_))) {
            self.net.profiles.need_profiles.insert(id.to_string());
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
    }
}
