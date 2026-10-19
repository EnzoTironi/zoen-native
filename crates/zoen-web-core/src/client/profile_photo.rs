//! Disposable verified profile image cache; ProfileFields remains the only authority.
use super::*;
use event_log::profile::{ProfilePhoto, MAX_PHOTO_BYTES};
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Save {
    name: String,
    bio: String,
    photo: Change,
    mime: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Change {
    Keep,
    Remove,
    Set,
}

impl<S: Signer> Client<S> {
    pub(super) fn profile_photo_cached(&self, photo: &ProfilePhoto) -> bool {
        photo.valid()
            && self
                .saved
                .profile
                .photos
                .get(&photo.sha256)
                .is_some_and(|bytes| {
                    !bytes.is_empty()
                        && bytes.len() <= MAX_PHOTO_BYTES
                        && hex::encode(Sha256::digest(bytes)) == photo.sha256
                })
    }

    fn current_photo(&self, identity: &str, expected: &str) -> Result<ProfilePhoto, String> {
        self.fields_of(identity)
            .and_then(|(_, fields)| fields.photo)
            .filter(|photo| photo.sha256 == expected && photo.valid())
            .ok_or_else(|| "Esta foto não faz mais parte do perfil disponível.".into())
    }

    pub fn profile_save(
        &mut self,
        request: &str,
        bytes: Vec<u8>,
        now: i64,
    ) -> Result<documents::DocumentResponse, String> {
        if request.len() > 4096 || bytes.len() > MAX_PHOTO_BYTES {
            return Err("A foto precisa ter até 5 MB.".into());
        }
        let save: Save = serde_json::from_str(request).map_err(|error| error.to_string())?;
        let name = save.name.trim();
        let bio = save.bio.trim();
        if name.is_empty() || name.chars().count() > 64 || bio.chars().count() > 280 {
            return Err("Confira seu nome e sua bio.".into());
        }
        let (_, _, mut fields) = self.my_profile_fields()?;
        let sealed = match save.photo {
            Change::Set => Some(
                ProfilePhoto::seal(&bytes, save.mime.as_deref().unwrap_or(""))
                    .ok_or("Foto inválida ou maior que 5 MB.")?,
            ),
            Change::Keep | Change::Remove if !bytes.is_empty() => {
                return Err("A alteração da foto não corresponde aos bytes enviados.".into());
            }
            _ => None,
        };
        fields.name = name.into();
        fields.bio = bio.into();
        match save.photo {
            Change::Remove => fields.photo = None,
            Change::Set => fields.photo = sealed.as_ref().map(|(photo, _)| photo.clone()),
            Change::Keep => {}
        }
        // Validate/version all fields before altering caches. The host durably saves this
        // one snapshot before any outgoing frame or upload, just like document edits.
        let previous = self.saved.profile.clone();
        let previous_profiles = self.saved.profiles.clone();
        let previous_name = self.account.name.clone();
        let added_blob = sealed.as_ref().map(|(photo, _)| photo.blob.clone());
        let previous_blob = added_blob
            .as_ref()
            .and_then(|blob| self.saved.local_chunks.get(blob).cloned());
        self.write_profile_fields(fields, false)?;
        if let Some((photo, sealed)) = sealed {
            self.saved.local_chunks.insert(photo.blob.clone(), sealed);
            self.saved.profile.photo_uploads.insert(photo.blob.clone());
            self.saved.profile.photos.insert(photo.sha256, bytes);
        }
        // Only the currently selected photo needs to hold this profile publication.
        let active = self
            .fields_of(&self.account.identity)
            .and_then(|(_, fields)| fields.photo)
            .map(|p| p.blob);
        self.saved
            .profile
            .photo_uploads
            .retain(|blob| Some(blob) == active.as_ref());
        self.view_now_ms = now;
        let update = match self.update(Vec::new()) {
            Ok(update) => update,
            Err(error) => {
                self.saved.profile = previous;
                self.saved.profiles = previous_profiles;
                self.account.name = previous_name;
                if let Some(blob) = added_blob {
                    if let Some(bytes) = previous_blob {
                        self.saved.local_chunks.insert(blob, bytes);
                    } else {
                        self.saved.local_chunks.remove(&blob);
                    }
                }
                return Err(error);
            }
        };
        Ok(documents::DocumentResponse {
            result: serde_json::Value::Null,
            update: Some(update),
            downloads: Vec::new(),
            uploads: self.pending_profile_uploads(),
        })
    }

    /// Small LE JSON header followed by cached pixels, never a JSON number array.
    pub fn profile_photo_framed(&self, identity: &str, expected: &str) -> Result<Vec<u8>, String> {
        let photo = self.current_photo(identity, expected)?;
        let bytes = self
            .saved
            .profile
            .photos
            .get(&photo.sha256)
            .filter(|_| self.profile_photo_cached(&photo));
        let header = serde_json::to_vec(&json!({"identity_id":identity,"sha256":photo.sha256,"mime":photo.mime,"blob":photo.blob,"cached":bytes.is_some()})).map_err(|error| error.to_string())?;
        let mut framed = Vec::with_capacity(4 + header.len() + bytes.map_or(0, Vec::len));
        framed.extend_from_slice(&(header.len() as u32).to_le_bytes());
        framed.extend_from_slice(&header);
        if let Some(bytes) = bytes {
            framed.extend_from_slice(bytes);
        }
        Ok(framed)
    }

    pub fn profile_photo_received(
        &mut self,
        identity: &str,
        expected: &str,
        sealed: Vec<u8>,
    ) -> Result<Update, String> {
        let photo = self.current_photo(identity, expected)?;
        let bytes = photo
            .open(&sealed)
            .ok_or("A foto recebida não corresponde ao perfil criptografado.")?;
        self.saved.profile.photos.insert(photo.sha256, bytes);
        self.update(Vec::new())
    }

    pub(super) fn pending_profile_uploads(&self) -> Vec<String> {
        self.saved.profile.photo_uploads.iter().cloned().collect()
    }
    pub(super) fn mark_profile_uploaded(&mut self, blobs: &[String]) {
        self.saved
            .profile
            .photo_uploads
            .retain(|blob| !blobs.contains(blob));
    }
}
