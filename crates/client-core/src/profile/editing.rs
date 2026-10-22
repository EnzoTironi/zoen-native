//! Explicit profile edits, photo preparation and block rotation.

use super::*;

impl Engine {
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

    pub(super) fn profile_photo(&mut self, bytes: &[u8], mime: &str) -> R<ProfilePhoto> {
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

    /// A fresh key from the next version on. Whoever had the old one keeps what it opened
    /// and nothing newer; everyone not blocked gets the new key in the next share pass.
    pub(super) fn rotate_profile_key(&mut self) -> R<()> {
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
}
