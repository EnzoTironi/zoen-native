// Existing native conversation surface, included at crate root for UniFFI identity.
#[uniffi::export]
impl Client {
    pub fn spaces(&self) -> Vec<SpaceSummary> {
        self.lock().spaces()
    }

    pub fn space(&self, space_id: String) -> Result<SpaceSummary, CoreError> {
        self.lock().space_summary(&space_id)
    }

    pub fn member_roles(&self, space_id: String) -> Result<Vec<MemberRoleDto>, CoreError> {
        self.lock().member_roles(&space_id)
    }

    pub fn timeline(&self, space_id: String) -> Result<Vec<TimelineEntry>, CoreError> {
        self.lock().timeline(&space_id)
    }

    /// Marks the chat read on this device and tells the others (blue ✓✓) and my other
    /// devices (their unread clears too) with one "read up to" watermark.
    pub fn mark_read(&self, space_id: String) -> Result<(), CoreError> {
        let signal = {
            let mut engine = self.lock();
            let seq = engine.mark_read(&space_id)?;
            if engine.net.synced.contains(&space_id) {
                seq.map(|seq| engine.read_time_announcement(&space_id, seq))
                    .transpose()?
            } else {
                None
            }
        };
        if let Some(kind) = signal {
            self.with_net(|n| n.ephemeral(space_id, kind));
        }
        Ok(())
    }

    /// My prefs for a chat (pin, archive, mute), synced to my other devices. Archiving keeps
    /// the chat out of the main list until I unarchive it; `muted_until_ms` silences
    /// notifications (`i64::MAX` = always, `None` = unmute).
    pub fn set_chat_prefs(
        &self,
        space_id: String,
        pinned: bool,
        archived: bool,
        muted_until_ms: Option<i64>,
    ) -> Result<(), CoreError> {
        let mut engine = self.lock();
        let mut prefs = engine
            .state
            .chat_prefs
            .get(&space_id)
            .cloned()
            .unwrap_or_default();
        prefs.pinned = pinned;
        prefs.archived = archived;
        prefs.muted_until_ms = muted_until_ms;
        engine.set_chat_prefs(&space_id, prefs)
    }

    /// Edits my message (only the author can).
    pub fn edit_message(
        &self,
        space_id: String,
        message_id: String,
        text: String,
    ) -> Result<(), CoreError> {
        self.lock()
            .annotate(&space_id, &message_id, types::Annotation::Edit { text })
    }

    /// Deletes my message for everyone (only the author can).
    pub fn delete_message(&self, space_id: String, message_id: String) -> Result<(), CoreError> {
        self.lock()
            .annotate(&space_id, &message_id, types::Annotation::Delete)
    }

    /// Sets my one reaction on a message (`None` removes it).
    pub fn react(
        &self,
        space_id: String,
        message_id: String,
        emoji: Option<String>,
    ) -> Result<(), CoreError> {
        let note = match emoji {
            Some(emoji) => types::Annotation::React { emoji },
            None => types::Annotation::Unreact,
        };
        self.lock().annotate(&space_id, &message_id, note)
    }

    pub fn forward_message(
        &self,
        from_space_id: String,
        message_id: String,
        to_space_id: String,
    ) -> Result<TimelineEntry, CoreError> {
        self.lock()
            .forward(&from_space_id, &message_id, &to_space_id)
    }

    pub fn send_message(&self, space_id: String, text: String) -> Result<TimelineEntry, CoreError> {
        self.lock().send_message(&space_id, &text)
    }

    /// Stores an attachment by content hash (sha256) and returns its reference.
    pub fn put_media(
        &self,
        bytes: Vec<u8>,
        mime: String,
        width: u32,
        height: u32,
    ) -> Result<MediaRefDto, CoreError> {
        self.lock().put_media(&bytes, &mime, width, height)
    }

    /// The attachment's bytes, verified against the hash (`None` if missing or tampered).
    pub fn media(&self, sha256: String) -> Result<Option<Vec<u8>>, CoreError> {
        self.lock().media(&sha256)
    }

    /// Sets the chat's background for everyone (a signed `BackgroundSet` event).
    pub fn set_background(
        &self,
        space_id: String,
        background: BackgroundDto,
    ) -> Result<TimelineEntry, CoreError> {
        self.lock().set_background(&space_id, &background)
    }

    /// The chat's current shared background, if any.
    pub fn background(&self, space_id: String) -> Result<Option<BackgroundDto>, CoreError> {
        self.lock().background(&space_id)
    }
}
