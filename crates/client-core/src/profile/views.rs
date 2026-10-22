//! Readable profile/photo projection and bounded download demand.

use super::*;

impl Engine {
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

    pub(super) fn apply_fields(&mut self, id: &str) -> R<()> {
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
                    maximum_sealed_bytes: Some(MAX_PHOTO_BYTES as u64 + 40),
                })
            })
            .collect()
    }
}
