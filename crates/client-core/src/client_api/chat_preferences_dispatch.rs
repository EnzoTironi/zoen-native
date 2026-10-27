//! Canonical chat-preference operations; old boolean callers remain compatible.
use super::{encode, invalid, Client, Command, CoreError};
impl Client {
    pub(super) fn dispatch_chat_preferences(&self, command: Command) -> Result<String, CoreError> {
        use Command::{SetChatMute, SetChatMuted, SetChatPrefs};
        match command {
            SetChatPrefs {
                space_id,
                pinned,
                archived,
                muted,
                preserve_mute,
            } => {
                if preserve_mute.unwrap_or(false) {
                    encode(
                        &self
                            .lock()
                            .set_chat_layout_prefs(&space_id, pinned, archived)?,
                    )
                } else {
                    encode(&self.set_chat_prefs(
                        space_id,
                        pinned,
                        archived,
                        muted.then_some(i64::MAX),
                    )?)
                }
            }
            SetChatMute {
                space_id,
                mode,
                expected_muted_until_ms,
                expected_identity_id,
                expected_device_id,
            } => encode(&self.lock().set_chat_mute(
                &space_id,
                &mode,
                expected_muted_until_ms.as_deref(),
                &expected_identity_id,
                &expected_device_id,
            )?),
            SetChatMuted {
                space_id,
                muted,
                expected_identity_id,
                expected_device_id,
            } => encode(&self.lock().set_chat_muted(
                &space_id,
                muted,
                &expected_identity_id,
                &expected_device_id,
            )?),
            _ => Err(invalid("Invalid chat preference command.")),
        }
    }
}
