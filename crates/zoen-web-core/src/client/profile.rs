//! Encrypted profile cache and retry journal. The wire codec and cryptography are
//! shared with native; the host persists every update before sending its frames.
use super::*;
use roda_log::profile::{self, AgreementKey, DeviceSigned, ProfileFields, ProfileKey, SealedShare};
use roda_proto::{AgreementKeyRecord, SealedProfile};
use serde_json::{json, Value};

const AGREEMENT_PUBLISH: u64 = 0x8000_0010;
const PROFILE_UPLOAD: u64 = 0x8000_0011;
const AGREEMENT_LOOKUP: u64 = 0x8000_0012;
const PROFILE_LOOKUP: u64 = 0x8000_0013;
const BATCH: usize = 500;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Journal {
    rows: BTreeMap<String, Row>,
    blocked: BTreeSet<String>,
    pub(super) agreement: BTreeMap<String, String>,
    published_agreement: String,
    uploaded: u64,
    upload: Option<Upload>,
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::super::tests::{account, clear, sequence, HostSigner};
    use super::*;
    use roda_log::{Author, Signer as NativeSigner};

    fn client() -> (Client<HostSigner>, NativeSigner, Author) {
        let identity = NativeSigner::from_secret(&[71; 32]);
        let device = NativeSigner::from_secret(&[72; 32]);
        let author = Author::device(&identity, device.clone());
        let mut account = account(&identity, &device);
        account.agreement_secret = hex::encode([73; 32]);
        account.profile_key = ProfileKey::generate().to_hex();
        account.profile_key_version = 1;
        account.profile_seen_version = 1;
        account.profile_fields = hex::encode(
            ProfileFields {
                name: "Antes".into(),
                bio: "bio".into(),
                photo: None,
            }
            .to_bytes(),
        );
        (
            Client::new(account, HostSigner(device), "").unwrap(),
            identity,
            author,
        )
    }

    fn synced(client: &mut Client<HostSigner>) -> Update {
        client.receive(&ServerFrame::SyncDone.encode(), 10).unwrap()
    }

    fn request(client: &mut Client<HostSigner>, command: Value) -> DocumentResponse {
        client.profile_request(&command.to_string(), 20).unwrap()
    }

    #[test]
    fn own_profile_uses_shared_cipher_signed_upload_and_retries_identical_bytes_after_reload() {
        let (mut client, identity, _) = client();
        request(
            &mut client,
            json!({"op":"update_profile","name":"  Depois 🦋  ","bio":"  privada  "}),
        );
        let update = synced(&mut client);
        let upload = update
            .frames
            .iter()
            .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
                ClientFrame::Req {
                    op: Op::PutProfile { profile },
                    ..
                } => Some(profile),
                _ => None,
            })
            .unwrap();
        assert!(profile::verify_upload(
            &identity.id(),
            upload.version,
            &upload.ciphertext,
            &upload.signed
        ));
        let (key, version, fields) = client.my_profile_fields().unwrap();
        assert_eq!(version, 2);
        assert_eq!(fields.name, "Depois 🦋");
        assert_eq!(fields.bio, "privada");
        assert_eq!(
            profile::open_profile(&key, &identity.id(), 2, &upload.ciphertext),
            Some(fields)
        );
        assert!(!upload
            .ciphertext
            .windows("Depois".len())
            .any(|bytes| bytes == b"Depois"));
        let mut reload = Client::new(
            client.account.clone(),
            client.signer.clone(),
            &update.snapshot,
        )
        .unwrap();
        let again = synced(&mut reload);
        let retry = again
            .frames
            .iter()
            .find_map(|frame| match ClientFrame::decode(frame).unwrap() {
                ClientFrame::Req {
                    op: Op::PutProfile { profile },
                    ..
                } => Some(profile),
                _ => None,
            })
            .unwrap();
        assert_eq!(retry, upload);
        assert_eq!(
            request(
                &mut reload,
                json!({"op":"profile","identity_id":identity.id()})
            )
            .result["version"],
            "2"
        );
    }

    #[test]
    fn handle_changes_redact_private_fields_and_only_apply_matching_acknowledgements() {
        let (mut client, identity, _) = client();
        let before = client.my_profile_fields().unwrap().2;
        let prepared = client.handle_profile("  @Depois_123  ").unwrap();
        let profile: Identity = serde_json::from_str(&prepared).unwrap();
        assert_eq!(profile.handle, "depois_123");
        assert!(profile.name.is_empty());
        assert!(profile.bio.is_empty());
        assert_eq!(client.account.handle, "fixture");
        let mut wrong = profile.clone();
        wrong.id = roda_log::Signer::from_secret(&[82; 32]).id();
        assert!(client
            .handle_confirmed(&serde_json::to_string(&wrong).unwrap(), "depois_123")
            .is_err());
        assert_eq!(client.account.handle, "fixture");
        assert!(client.handle_confirmed(&prepared, "different").is_err());
        assert_eq!(client.account.handle, "fixture");
        let update = client.handle_confirmed(&prepared, "depois_123").unwrap();
        assert_eq!(client.account.handle, "depois_123");
        assert_eq!(client.my_profile_fields().unwrap().2, before);
        let mut original = client.account.clone();
        original.handle = "fixture".into();
        let reload = Client::new(original, client.signer.clone(), &update.snapshot).unwrap();
        assert_eq!(reload.account.handle, "depois_123");
        assert_eq!(reload.fields_of(&identity.id()).unwrap().1, before);
    }

    #[test]
    fn certified_native_share_opens_only_matching_signed_profile_and_survives_reload() {
        let (mut client, _, _) = client();
        let identity = NativeSigner::from_secret(&[74; 32]);
        let device = NativeSigner::from_secret(&[75; 32]);
        let author = Author::device(&identity, device);
        let key = ProfileKey::generate();
        let agreement = client.agreement_key().unwrap();
        let space = "sp_profile";
        let mut log = SpaceLog::new(space);
        let create = clear(
            &mut log,
            &author,
            EventBody::SpaceCreated {
                title: "Profile".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
            1,
        );
        client.receive(&create, 2).unwrap();
        let share = profile::seal_share(
            &key,
            3,
            &agreement.public_hex(),
            &profile::share_context(space, &identity.id(), &client.account.identity),
        )
        .unwrap();
        let share = clear(
            &mut log,
            &author,
            EventBody::ProfileKeyShared {
                version: 3,
                shares: vec![roda_types::ProfileKeyShare {
                    to: client.account.identity.clone(),
                    ephemeral: share.ephemeral,
                    sealed: share.sealed,
                }],
            },
            3,
        );
        client.receive(&share, 4).unwrap();
        let update = synced(&mut client);
        assert!(update.frames.iter().any(|frame| matches!(
            ClientFrame::decode(frame),
            Ok(ClientFrame::Req {
                op: Op::GetProfiles { .. },
                ..
            })
        )));
        let fields = ProfileFields {
            name: "Nome privado".into(),
            bio: "Só contatos".into(),
            photo: None,
        };
        let ciphertext = profile::seal_profile(&key, &identity.id(), 3, &fields);
        let signed = profile::sign_upload(&author, 3, &ciphertext).unwrap();
        let mut forged = SealedProfile {
            identity: identity.id(),
            version: 4,
            ciphertext: ciphertext.clone(),
            signed: signed.clone(),
        };
        client
            .receive(
                &ServerFrame::Res {
                    id: PROFILE_LOOKUP,
                    result: Ok(Reply::SealedProfiles(vec![forged.clone()])),
                }
                .encode(),
                11,
            )
            .unwrap();
        assert!(client.fields_of(&identity.id()).is_none());
        client.profile_changed(&identity.id());
        client.update(Vec::new()).unwrap();
        forged.version = 3;
        let update = client
            .receive(
                &ServerFrame::Res {
                    id: PROFILE_LOOKUP,
                    result: Ok(Reply::SealedProfiles(vec![forged])),
                }
                .encode(),
                12,
            )
            .unwrap();
        let reload = Client::new(
            client.account.clone(),
            client.signer.clone(),
            &update.snapshot,
        )
        .unwrap();
        assert_eq!(reload.fields_of(&identity.id()), Some((3, fields)));
        assert!(
            request(
                &mut client,
                json!({"op":"profile","identity_id":identity.id()})
            )
            .result["name"]
                == "Nome privado"
        );
    }

    #[test]
    fn failed_recovery_key_lookup_retries_without_downgrading_blocked_member_to_legacy() {
        let (mut client, _, author) = client();
        let other = NativeSigner::from_secret(&[84; 32]);
        let mut log = SpaceLog::new("sp_key_retry");
        client
            .receive(
                &clear(
                    &mut log,
                    &author,
                    EventBody::SpaceCreated {
                        title: "Keys".into(),
                        kind: SpaceKind::Group,
                        privacy: Privacy::Closed,
                    },
                    1,
                ),
                2,
            )
            .unwrap();
        client
            .receive(
                &clear(
                    &mut log,
                    &author,
                    EventBody::MemberAdded {
                        identity: other.id(),
                        role: Role::Member,
                    },
                    3,
                ),
                4,
            )
            .unwrap();
        client.saved.profile.blocked.insert(other.id());
        let lookup = |update: &Update| {
            update.frames.iter().any(|frame|matches!(ClientFrame::decode(frame).unwrap(),ClientFrame::Req{op:Op::AgreementKeys{ids},..} if ids.contains(&other.id())))
        };
        assert!(
            lookup(&synced(&mut client)),
            "a blocked member still needs an authenticated MLS recovery key"
        );
        let failed = client
            .receive(
                &ServerFrame::Res {
                    id: AGREEMENT_LOOKUP,
                    result: Err("temporary failure".into()),
                }
                .encode(),
                11,
            )
            .unwrap();
        assert!(!client.profile_net.no_agreement.contains(&other.id()));
        assert!(!lookup(&failed));
        assert!(!lookup(&client.maintenance(5010).unwrap()));
        assert!(lookup(&client.maintenance(5011).unwrap()));
        client
            .receive(
                &ServerFrame::Res {
                    id: AGREEMENT_LOOKUP,
                    result: Ok(Reply::AgreementKeys(Vec::new())),
                }
                .encode(),
                5012,
            )
            .unwrap();
        assert!(
            client.profile_net.no_agreement.contains(&other.id()),
            "only a successful empty answer establishes legacy absence"
        );
    }

    #[test]
    fn blocking_rotates_profile_key_and_never_reshares_to_blocked_person() {
        let (mut client, _, author) = client();
        let other = NativeSigner::from_secret(&[76; 32]);
        let space = "sp_block";
        let mut log = SpaceLog::new(space);
        let create = clear(
            &mut log,
            &author,
            EventBody::SpaceCreated {
                title: "Block".into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
            1,
        );
        client.receive(&create, 2).unwrap();
        let add = clear(
            &mut log,
            &author,
            EventBody::MemberAdded {
                identity: other.id(),
                role: Role::Member,
            },
            3,
        );
        client.receive(&add, 4).unwrap();
        client
            .saved
            .profile
            .agreement
            .insert(other.id(), AgreementKey::generate().public_hex());
        let old = client.my_profile_fields().unwrap().0;
        request(
            &mut client,
            json!({"op":"block_person","identity_id":other.id()}),
        );
        assert_ne!(old.to_hex(), client.my_profile_fields().unwrap().0.to_hex());
        let update = synced(&mut client);
        for frame in &update.frames {
            if let ClientFrame::Publish { env } = ClientFrame::decode(frame).unwrap() {
                let event = roda_log::event_from_content(
                    env.content().to_vec(),
                    env.sig,
                    env.cert,
                    0,
                    String::new(),
                    String::new(),
                )
                .unwrap();
                if let EventBody::ProfileKeyShared { shares, .. } = event.body {
                    assert!(shares.iter().all(|share| share.to != other.id()));
                }
            }
        }
        let update = request(
            &mut client,
            json!({"op":"unblock_person","identity_id":other.id()}),
        )
        .update
        .unwrap();
        let mut found = false;
        for frame in update.frames {
            if let ClientFrame::Publish { env } = ClientFrame::decode(&frame).unwrap() {
                let bytes = sequence(&mut log, env);
                let decoded = ServerFrame::decode(&bytes).unwrap();
                if let ServerFrame::Event { ev } = decoded {
                    let event = roda_log::event_from_content(
                        ev.env.content().to_vec(),
                        ev.env.sig,
                        ev.env.cert,
                        0,
                        String::new(),
                        String::new(),
                    )
                    .unwrap();
                    if let EventBody::ProfileKeyShared { shares, .. } = event.body {
                        found |= shares.iter().any(|share| share.to == other.id());
                    }
                }
            }
        }
        assert!(found);
    }
}

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
        if identity != self.account.identity {
            self.profile_net.fetched.remove(identity);
        }
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
        self.overlay_saved_profiles();
    }

    /// Prepare the redacted registration payload without changing the local alias.
    pub fn handle_profile(&self, handle: &str) -> Result<String, String> {
        let handle = roda_proto::normalize_handle(handle)
            .ok_or("Use de 3 a 24 letras, números, pontos ou _. Comece com uma letra.")?;
        let mut profile = self
            .saved
            .profiles
            .get(&self.account.identity)
            .cloned()
            .unwrap_or(Identity {
                id: self.account.identity.clone(),
                kind: roda_types::IdentityKind::Person,
                name: String::new(),
                handle: handle.clone(),
                tint_hex: "#4F7CFF".into(),
                glyph: None,
                owner: None,
                bio: String::new(),
                owner_proof: None,
            });
        if profile.kind != roda_types::IdentityKind::Person {
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
        let expected = roda_proto::normalize_handle(expected).ok_or("Nome de usuário inválido.")?;
        if profile.id != self.account.identity
            || profile.kind != roda_types::IdentityKind::Person
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

    fn my_profile_fields(&self) -> Result<(ProfileKey, u64, ProfileFields), String> {
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
                .filter(|person| person.kind == roda_types::IdentityKind::Person)
            {
                person.name = fields.name.clone();
                person.bio = fields.bio.clone();
            }
            if *id == self.account.identity {
                self.account.name = fields.name;
            }
        }
    }

    fn write_profile_fields(&mut self, fields: ProfileFields, rotate: bool) -> Result<(), String> {
        let me = self.account.identity.clone();
        let row = self
            .saved
            .profile
            .rows
            .get_mut(&me)
            .ok_or("Perfil indisponível.")?;
        let next = row
            .seen_version
            .checked_add(1)
            .ok_or("O perfil atingiu o limite de versões.")?;
        if rotate {
            row.key = ProfileKey::generate().to_hex();
            row.key_version = next;
        }
        row.seen_version = next;
        row.fields = Some(fields.to_bytes());
        self.saved.profile.upload = None;
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
                    || roda_log::parse_device_key(&identity_id).is_none()
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
        let agent =
            identity.is_some_and(|identity| identity.kind == roda_types::IdentityKind::Agent);
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
            "name":name,"bio":bio,"photo_sha256":photo.map(|photo|photo.sha256),"photo_ready":false,"version":version.to_string(),
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

    pub(super) fn profile_reply(
        &mut self,
        id: u64,
        result: &Result<Reply, String>,
    ) -> Result<bool, String> {
        match id {
            AGREEMENT_PUBLISH => {
                self.profile_net.publish = false;
                if matches!(result, Ok(Reply::Done)) {
                    self.saved.profile.published_agreement = self
                        .agreement_key()
                        .map(|key| key.public_hex())
                        .unwrap_or_default();
                } else {
                    self.profile_net.publish = true;
                }
            }
            PROFILE_UPLOAD => {
                self.profile_net.upload = false;
                if matches!(result, Ok(Reply::Done)) {
                    if let Some(upload) = self.saved.profile.upload.take() {
                        self.saved.profile.uploaded = upload.version;
                    }
                } else if result
                    .as_ref()
                    .err()
                    .is_some_and(|error| error.contains("stale profile version"))
                {
                    let (_, _, fields) = self.my_profile_fields()?;
                    self.write_profile_fields(fields, false)?;
                } else {
                    self.profile_net.upload_failed = true;
                }
            }
            AGREEMENT_LOOKUP => {
                let asked = std::mem::take(&mut self.profile_net.agreement);
                self.profile_net.agreement_deadline = 0;
                let records: &[AgreementKeyRecord] = match result {
                    Ok(Reply::AgreementKeys(records)) => records,
                    _ => {
                        self.profile_net.agreement_retry = self.view_now_ms.saturating_add(5000);
                        return Ok(true);
                    }
                };
                let mut found = BTreeSet::new();
                for record in records
                    .iter()
                    .filter(|record| asked.contains(&record.identity))
                {
                    if profile::verify_agreement(&record.identity, &record.public, &record.signed) {
                        self.saved
                            .profile
                            .agreement
                            .insert(record.identity.clone(), record.public.clone());
                        found.insert(record.identity.clone());
                    }
                }
                self.profile_net
                    .no_agreement
                    .extend(asked.into_iter().filter(|id| !found.contains(id)));
            }
            PROFILE_LOOKUP => {
                let asked = std::mem::take(&mut self.profile_net.profiles);
                self.profile_net.fetched.extend(asked.iter().cloned());
                let records: &[SealedProfile] = match result {
                    Ok(Reply::SealedProfiles(records)) => records,
                    _ => &[],
                };
                for record in records
                    .iter()
                    .filter(|record| asked.contains(&record.identity))
                {
                    let Some(row) = self.saved.profile.rows.get_mut(&record.identity) else {
                        continue;
                    };
                    if record.ciphertext.len() > profile::MAX_CIPHERTEXT
                        || record.version <= row.seen_version
                        || record.version < row.key_version
                        || !profile::verify_upload(
                            &record.identity,
                            record.version,
                            &record.ciphertext,
                            &record.signed,
                        )
                    {
                        continue;
                    }
                    let Some(key) = ProfileKey::from_hex(&row.key) else {
                        continue;
                    };
                    if let Some(fields) = profile::open_profile(
                        &key,
                        &record.identity,
                        record.version,
                        &record.ciphertext,
                    ) {
                        row.seen_version = record.version;
                        row.fields = Some(fields.to_bytes());
                    }
                }
                self.overlay_saved_profiles();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(super) fn profile_deadline(&self) -> Option<i64> {
        [
            self.profile_net.agreement_retry,
            self.profile_net.agreement_deadline,
        ]
        .into_iter()
        .filter(|deadline| *deadline > self.view_now_ms)
        .min()
    }

    pub(super) fn profile_frames(&mut self, frames: &mut Vec<Vec<u8>>) -> Result<(), String> {
        if !self.synced || self.agreement_key().is_none() || self.my_profile_fields().is_err() {
            return Ok(());
        }
        if !self.profile_net.publish {
            let public = self.agreement_key().expect("checked").public_hex();
            if self.saved.profile.published_agreement != public {
                let signed = self.signed_profile_statement(&profile::agreement_message(
                    &self.account.identity,
                    &public,
                ))?;
                frames.push(
                    ClientFrame::Req {
                        id: AGREEMENT_PUBLISH,
                        op: Op::PublishAgreementKey { public, signed },
                    }
                    .encode(),
                );
                self.profile_net.publish = true;
            }
        }
        let (key, version, fields) = self.my_profile_fields()?;
        if !self.profile_net.upload
            && !self.profile_net.upload_failed
            && version > self.saved.profile.uploaded
        {
            if self
                .saved
                .profile
                .upload
                .as_ref()
                .is_none_or(|upload| upload.version != version)
            {
                let ciphertext =
                    profile::seal_profile(&key, &self.account.identity, version, &fields);
                let signed = self.signed_profile_statement(&profile::upload_message(
                    &self.account.identity,
                    version,
                    &ciphertext,
                ))?;
                self.saved.profile.upload = Some(Upload {
                    version,
                    ciphertext,
                    signature: signed.sig,
                });
            }
            let upload = self.saved.profile.upload.as_ref().expect("prepared");
            frames.push(
                ClientFrame::Req {
                    id: PROFILE_UPLOAD,
                    op: Op::PutProfile {
                        profile: SealedProfile {
                            identity: self.account.identity.clone(),
                            version: upload.version,
                            ciphertext: upload.ciphertext.clone(),
                            signed: DeviceSigned {
                                device: self.account.device.clone(),
                                cert: self.account.cert.clone(),
                                sig: upload.signature.clone(),
                            },
                        },
                    },
                }
                .encode(),
            );
            self.profile_net.upload = true;
        }
        self.share_profile_key()?;
        if !self.profile_net.agreement.is_empty()
            && self.profile_net.agreement_deadline <= self.view_now_ms
        {
            self.profile_net.agreement.clear();
        }
        if self.profile_net.agreement.is_empty()
            && self.profile_net.agreement_retry <= self.view_now_ms
        {
            let missing = self
                .logs
                .keys()
                .flat_map(|space| self.roster(space).into_keys())
                .filter(|id| {
                    id != &self.account.identity
                        && !self.saved.profile.agreement.contains_key(id)
                        && !self.profile_net.no_agreement.contains(id)
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(BATCH)
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                frames.push(
                    ClientFrame::Req {
                        id: AGREEMENT_LOOKUP,
                        op: Op::AgreementKeys {
                            ids: missing.clone(),
                        },
                    }
                    .encode(),
                );
                self.profile_net.agreement = missing;
                self.profile_net.agreement_deadline = self.view_now_ms.saturating_add(15_000);
            }
        }
        if self.profile_net.profiles.is_empty() {
            let missing = self
                .saved
                .profile
                .rows
                .keys()
                .filter(|id| {
                    *id != &self.account.identity && !self.profile_net.fetched.contains(*id)
                })
                .take(BATCH)
                .cloned()
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                frames.push(
                    ClientFrame::Req {
                        id: PROFILE_LOOKUP,
                        op: Op::GetProfiles {
                            ids: missing.clone(),
                        },
                    }
                    .encode(),
                );
                self.profile_net.profiles = missing;
            }
        }
        Ok(())
    }

    fn share_profile_key(&mut self) -> Result<(), String> {
        let row = self
            .saved
            .profile
            .rows
            .get(&self.account.identity)
            .ok_or("Perfil indisponível.")?;
        let key = ProfileKey::from_hex(&row.key).ok_or("Chave de perfil inválida.")?;
        let version = row.key_version;
        let me = &self.account.identity;
        let mut shared = BTreeSet::new();
        let pending = self
            .saved
            .outbox
            .values()
            .filter(|pending| pending.failure.is_none())
            .filter_map(|pending| pending.envelope().ok())
            .filter_map(|envelope| {
                roda_log::event_from_content(
                    envelope.content().to_vec(),
                    envelope.sig,
                    envelope.cert,
                    0,
                    String::new(),
                    String::new(),
                )
                .ok()
            })
            .collect::<Vec<_>>();
        for event in self
            .logs
            .values()
            .flat_map(|log| log.events())
            .chain(pending.iter())
        {
            if let EventBody::ProfileKeyShared { version: v, shares } = &event.body {
                if event.author == *me && *v == version {
                    shared.extend(shares.iter().map(|share| share.to.clone()));
                }
            }
        }
        let mut bodies = Vec::new();
        for space in self.logs.keys() {
            let roster = self.roster(space);
            if !roster.contains_key(me) {
                continue;
            }
            let mut shares = Vec::new();
            for id in roster.keys() {
                if id == me
                    || shared.contains(id)
                    || self.saved.profile.blocked.contains(id)
                    || self
                        .saved
                        .profiles
                        .get(id)
                        .is_some_and(|identity| identity.kind == roda_types::IdentityKind::Agent)
                {
                    continue;
                }
                let Some(public) = self.saved.profile.agreement.get(id) else {
                    continue;
                };
                if let Some(sealed) = profile::seal_share(
                    &key,
                    version,
                    public,
                    &profile::share_context(space, me, id),
                ) {
                    shares.push(roda_types::ProfileKeyShare {
                        to: id.clone(),
                        ephemeral: sealed.ephemeral,
                        sealed: sealed.sealed,
                    });
                    shared.insert(id.clone());
                }
            }
            for shares in shares.chunks(200) {
                bodies.push((
                    space.clone(),
                    EventBody::ProfileKeyShared {
                        version,
                        shares: shares.to_vec(),
                    },
                ));
            }
        }
        for (space, body) in bodies {
            let pending = self.clear_event(&space, body, self.view_now_ms)?;
            let id = pending.envelope()?.client_id().to_string();
            self.saved.outbox.insert(id, pending);
        }
        Ok(())
    }
}
