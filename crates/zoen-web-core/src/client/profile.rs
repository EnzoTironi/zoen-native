//! Encrypted profile cache and retry journal. The wire codec and cryptography are
//! shared with native; the host persists every update before sending its frames.
use super::*;
use event_log::profile::{
    self, AgreementKey, DeviceSigned, ProfileFields, ProfileKey, SealedShare,
};
use event_log::profile_sync::{Observe, OwnProfileSync};
use protocol::{AgreementKeyRecord, SealedProfile};
use serde_json::{json, Value};

const AGREEMENT_PUBLISH: u64 = 0x8000_0010;
const AGREEMENT_LOOKUP: u64 = 0x8000_0012;
const BATCH: usize = 500;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Journal {
    rows: BTreeMap<String, Row>,
    blocked: BTreeSet<String>,
    pub(super) agreement: BTreeMap<String, String>,
    published_agreement: String,
    #[serde(default, skip_serializing)]
    uploaded: u64, // read once when migrating an existing journal
    #[serde(default)]
    own_sync: Option<OwnProfileSync>,
    upload: Option<Upload>,
    #[serde(default)]
    pub(super) photos: BTreeMap<String, Vec<u8>>,
    #[serde(default)]
    pub(super) photo_uploads: BTreeSet<String>,
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;

#[derive(Clone, Serialize, Deserialize)]
struct Row {
    key: String,
    key_version: u64,
    seen_version: u64,
    fields: Option<Vec<u8>>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Upload {
    version: u64,
    ciphertext: Vec<u8>,
    signature: String,
}

#[derive(Default)]
pub(super) struct Network {
    publish: bool,
    upload: bool,
    upload_failed: bool,
    agreement: Vec<String>,
    agreement_retry: i64,
    agreement_deadline: i64,
    profiles: Vec<String>,
    pub(super) no_agreement: BTreeSet<String>,
    fetched: BTreeSet<String>,
    profiles_request: Option<u64>,
    profiles_retry: i64,
    profiles_deadline: i64,
    refetch: BTreeSet<String>,
    upload_version: Option<u64>,
    upload_request: Option<u64>,
    upload_deadline: i64,
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Profile { identity_id: String },
    UpdateProfile { name: String, bio: String },
    BlockPerson { identity_id: String },
    UnblockPerson { identity_id: String },
    BlockedPeople {},
}

impl<S: Signer> Client<S> {
    pub(super) fn profile_changed(&mut self, identity: &str) {
        if self.profile_net.profiles.iter().any(|id| id == identity) {
            self.profile_net.refetch.insert(identity.into());
        }
        self.profile_net.fetched.remove(identity);
    }
    pub(super) fn load_linked_profile(&mut self) {
        if let Some(handle) = &self.saved.own_handle {
            self.account.handle = handle.clone();
        }
        if !self.saved.profile.rows.contains_key(&self.account.identity)
            && ProfileKey::from_hex(&self.account.profile_key).is_some()
        {
            let fields = hex::decode(&self.account.profile_fields)
                .ok()
                .filter(|bytes| ProfileFields::from_bytes(bytes).is_some())
                .unwrap_or_else(|| {
                    ProfileFields {
                        name: self.account.name.clone(),
                        ..Default::default()
                    }
                    .to_bytes()
                });
            self.saved.profile.own_sync = Some(if self.account.primary == self.account.device {
                OwnProfileSync::migrate(self.account.profile_seen_version, 0)
            } else {
                OwnProfileSync::default()
            });
            self.saved.profile.rows.insert(
                self.account.identity.clone(),
                Row {
                    key: self.account.profile_key.clone(),
                    key_version: self.account.profile_key_version,
                    seen_version: self.account.profile_seen_version,
                    fields: Some(fields),
                },
            );
        }
        if self.saved.profile.own_sync.is_none() {
            if let Some(row) = self.saved.profile.rows.get(&self.account.identity) {
                self.saved.profile.own_sync = Some(OwnProfileSync::migrate(
                    row.seen_version,
                    self.saved.profile.uploaded,
                ));
            }
        }
        self.overlay_saved_profiles();
    }

    /// Prepare the redacted registration payload without changing the local alias.
    pub fn handle_profile(&self, handle: &str) -> Result<String, String> {
        let handle = protocol::normalize_handle(handle)
            .ok_or("Use de 3 a 24 letras, números, pontos ou _. Comece com uma letra.")?;
        let mut profile = self
            .saved
            .profiles
            .get(&self.account.identity)
            .cloned()
            .unwrap_or(Identity {
                id: self.account.identity.clone(),
                kind: types::IdentityKind::Person,
                name: String::new(),
                handle: handle.clone(),
                tint_hex: "#4F7CFF".into(),
                glyph: None,
                owner: None,
                bio: String::new(),
                owner_proof: None,
            });
        if profile.kind != types::IdentityKind::Person {
            return Err("Este perfil não é uma pessoa.".into());
        }
        profile.name.clear();
        profile.bio.clear();
        profile.handle = handle;
        serde_json::to_string(&profile).map_err(|error| error.to_string())
    }

    /// Only a matching Registered response on this authenticated session can rename.
    pub fn handle_confirmed(&mut self, encoded: &str, expected: &str) -> Result<Update, String> {
        let mut profile: Identity =
            serde_json::from_str(encoded).map_err(|error| error.to_string())?;
        let expected = protocol::normalize_handle(expected).ok_or("Nome de usuário inválido.")?;
        if profile.id != self.account.identity
            || profile.kind != types::IdentityKind::Person
            || profile.owner.is_some()
            || profile.owner_proof.is_some()
            || profile.handle != expected
        {
            return Err("O servidor confirmou outro perfil. Tente novamente.".into());
        }
        self.account.handle = expected.clone();
        self.saved.own_handle = Some(expected);
        profile.name.clear();
        profile.bio.clear();
        self.saved.profiles.insert(profile.id.clone(), profile);
        self.overlay_saved_profiles();
        self.update(Vec::new())
    }

    pub(super) fn agreement_key(&self) -> Option<AgreementKey> {
        let secret: [u8; 32] = hex::decode(&self.account.agreement_secret)
            .ok()?
            .try_into()
            .ok()?;
        Some(AgreementKey::from_secret(secret))
    }

    pub(super) fn fields_of(&self, id: &str) -> Option<(u64, ProfileFields)> {
        let row = self.saved.profile.rows.get(id)?;
        Some((
            row.seen_version,
            ProfileFields::from_bytes(row.fields.as_deref()?)?,
        ))
    }

    pub(super) fn profile_link_fields(&self) -> Result<(String, u64, u64, String), String> {
        let row = self
            .saved
            .profile
            .rows
            .get(&self.account.identity)
            .ok_or("Perfil indisponível.")?;
        ProfileKey::from_hex(&row.key).ok_or("Chave de perfil inválida.")?;
        let fields = row.fields.as_deref().ok_or("Perfil indisponível.")?;
        ProfileFields::from_bytes(fields).ok_or("Perfil inválido.")?;
        Ok((
            row.key.clone(),
            row.key_version,
            row.seen_version,
            hex::encode(fields),
        ))
    }

    pub(super) fn my_profile_fields(&self) -> Result<(ProfileKey, u64, ProfileFields), String> {
        let row = self
            .saved
            .profile
            .rows
            .get(&self.account.identity)
            .ok_or("Perfil indisponível. Vincule este navegador novamente.")?;
        Ok((
            ProfileKey::from_hex(&row.key).ok_or("Chave de perfil inválida.")?,
            row.seen_version,
            ProfileFields::from_bytes(row.fields.as_deref().ok_or("Perfil indisponível.")?)
                .ok_or("Perfil inválido.")?,
        ))
    }

    pub(super) fn overlay_saved_profiles(&mut self) {
        for (id, row) in &self.saved.profile.rows {
            let Some(fields) = row.fields.as_deref().and_then(ProfileFields::from_bytes) else {
                continue;
            };
            if let Some(person) = self
                .saved
                .profiles
                .get_mut(id)
                .filter(|person| person.kind == types::IdentityKind::Person)
            {
                person.name = fields.name.clone();
                person.bio = fields.bio.clone();
            }
            if *id == self.account.identity {
                self.account.name = fields.name;
            }
        }
    }

    pub(super) fn write_profile_fields(
        &mut self,
        fields: ProfileFields,
        rotate: bool,
    ) -> Result<(), String> {
        let me = self.account.identity.clone();
        let row = self
            .saved
            .profile
            .rows
            .get_mut(&me)
            .ok_or("Perfil indisponível.")?;
        let rotated_key = if rotate {
            Some(ProfileKey::generate().map_err(|error| error.to_string())?)
        } else {
            None
        };
        let sync = self
            .saved
            .profile
            .own_sync
            .as_mut()
            .ok_or("Estado do perfil indisponível.")?;
        let next = sync
            .edit(row.seen_version)
            .ok_or("O perfil atingiu o limite de versões.")?;
        if let Some(key) = rotated_key {
            row.key = key.to_hex();
            row.key_version = next;
        }
        row.seen_version = next;
        row.fields = Some(fields.to_bytes());
        if !self.profile_net.upload {
            self.saved.profile.upload = None;
        }
        self.profile_net.upload_failed = false;
        self.overlay_saved_profiles();
        Ok(())
    }

    fn signed_profile_statement(&self, bytes: &[u8]) -> Result<DeviceSigned, String> {
        Ok(DeviceSigned {
            device: self.account.device.clone(),
            cert: self.account.cert.clone(),
            sig: hex::encode(
                self.signer
                    .sign(bytes)
                    .map_err(|error| format!("{error:?}"))?,
            ),
        })
    }

    pub fn profile_request(&mut self, request: &str, now: i64) -> Result<DocumentResponse, String> {
        self.view_now_ms = now;
        if request.len() > 16 * 1024 {
            return Err("Perfil grande demais.".into());
        }
        let command: Command = serde_json::from_str(request).map_err(|error| error.to_string())?;
        let mut changed = false;
        let result = match command {
            Command::Profile { identity_id } => self.profile_value(&identity_id)?,
            Command::BlockedPeople {} => json!(self.saved.profile.blocked),
            Command::UpdateProfile { name, bio } => {
                let name = name.trim();
                let bio = bio.trim();
                if name.is_empty() || name.chars().count() > 64 {
                    return Err("Diga seu nome com até 64 caracteres.".into());
                }
                if bio.chars().count() > 280 {
                    return Err("A bio tem no máximo 280 caracteres.".into());
                }
                let (_, _, mut fields) = self.my_profile_fields()?;
                if fields.name != name || fields.bio != bio {
                    fields.name = name.into();
                    fields.bio = bio.into();
                    self.write_profile_fields(fields, false)?;
                    changed = true;
                }
                Value::Null
            }
            Command::BlockPerson { identity_id } => {
                if identity_id == self.account.identity
                    || event_log::parse_device_key(&identity_id).is_none()
                {
                    return Err("Não dá para bloquear essa pessoa.".into());
                }
                if !self.saved.profile.blocked.contains(&identity_id) {
                    let (_, _, fields) = self.my_profile_fields()?;
                    self.write_profile_fields(fields, true)?;
                    self.saved.profile.blocked.insert(identity_id);
                    changed = true;
                }
                Value::Null
            }
            Command::UnblockPerson { identity_id } => {
                changed = self.saved.profile.blocked.remove(&identity_id);
                Value::Null
            }
        };
        Ok(DocumentResponse {
            result,
            update: if changed {
                Some(self.update(Vec::new())?)
            } else {
                None
            },
            downloads: Vec::new(),
            uploads: Vec::new(),
        })
    }

    fn profile_value(&self, id: &str) -> Result<Value, String> {
        let identity = self.saved.profiles.get(id);
        let fields = self.fields_of(id);
        if identity.is_none() && fields.is_none() && id != self.account.identity {
            return Err("Perfil não encontrado.".into());
        }
        let agent = identity.is_some_and(|identity| identity.kind == types::IdentityKind::Agent);
        let (name, bio, version, photo) = if agent {
            (
                identity.map(|identity| identity.name.clone()),
                identity.map(|identity| identity.bio.clone()),
                0,
                None,
            )
        } else if let Some((version, fields)) = fields {
            (Some(fields.name), Some(fields.bio), version, fields.photo)
        } else {
            (None, None, 0, None)
        };
        Ok(
            json!({"identity_id":id,"handle":identity.map(|identity|identity.handle.as_str()).unwrap_or(if id==self.account.identity {&self.account.handle} else {""}),
            "name":name,"bio":bio,"photo_sha256":photo.as_ref().map(|photo|photo.sha256.clone()),"photo_mime":photo.as_ref().map(|photo|photo.mime.clone()),"photo_ready":photo.as_ref().is_some_and(|photo| self.profile_photo_cached(photo)),"version":version.to_string(),
            "tint_hex":identity.map(|identity|identity.tint_hex.as_str()).unwrap_or("#8E8E93"),"is_me":id==self.account.identity,"blocked":self.saved.profile.blocked.contains(id)}),
        )
    }

    pub(super) fn note_profile_event(&mut self, event: &Event) {
        let EventBody::ProfileKeyShared { shares, .. } = &event.body else {
            return;
        };
        if event.author == self.account.identity {
            return;
        }
        let Some(agreement) = self.agreement_key() else {
            return;
        };
        let Some(share) = shares
            .iter()
            .find(|share| share.to == self.account.identity)
        else {
            return;
        };
        let Some((key, version)) = profile::open_share(
            &agreement,
            &SealedShare {
                ephemeral: share.ephemeral.clone(),
                sealed: share.sealed.clone(),
            },
            &profile::share_context(&event.space, &event.author, &self.account.identity),
        ) else {
            return;
        };
        if self
            .saved
            .profile
            .rows
            .get(&event.author)
            .is_some_and(|row| row.key_version >= version)
        {
            return;
        }
        let previous = self.saved.profile.rows.get(&event.author);
        self.saved.profile.rows.insert(
            event.author.clone(),
            Row {
                key: key.to_hex(),
                key_version: version,
                seen_version: previous.map(|row| row.seen_version).unwrap_or(0),
                fields: previous.and_then(|row| row.fields.clone()),
            },
        );
        self.profile_net.fetched.remove(&event.author);
    }
}

mod lookup;
mod publication;
