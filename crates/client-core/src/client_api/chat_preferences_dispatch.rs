//! Canonical chat-preference operations; old boolean callers remain compatible.
use super::{encode, invalid, Client, Command, CoreError};
use serde::Deserialize;
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(super) struct ChatPrefsInput {
    space_id: String,
    pinned: bool,
    archived: bool,
    muted: bool,
    #[serde(default)]
    #[cfg_attr(feature = "client-types", ts(optional))]
    preserve_mute: Option<bool>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(super) struct ChatMuteInput {
    space_id: String,
    mode: crate::ChatMuteMode,
    expected_muted_until_ms: Option<String>,
    expected_identity_id: String,
    expected_device_id: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub(super) struct ChatMutedInput {
    space_id: String,
    muted: bool,
    expected_identity_id: String,
    expected_device_id: String,
}
impl Client {
    pub(super) fn dispatch_chat_preferences(&self, command: Command) -> Result<String, CoreError> {
        use Command::{ChatLayout, SetChatMute, SetChatMuted, SetChatPrefs};
        match command {
            SetChatPrefs(ChatPrefsInput {
                space_id,
                pinned,
                archived,
                muted,
                preserve_mute,
            }) => {
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
            SetChatMute(ChatMuteInput {
                space_id,
                mode,
                expected_muted_until_ms,
                expected_identity_id,
                expected_device_id,
            }) => encode(&self.lock().set_chat_mute(
                &space_id,
                &mode,
                expected_muted_until_ms.as_deref(),
                &expected_identity_id,
                &expected_device_id,
            )?),
            SetChatMuted(ChatMutedInput {
                space_id,
                muted,
                expected_identity_id,
                expected_device_id,
            }) => encode(&self.lock().set_chat_muted(
                &space_id,
                muted,
                &expected_identity_id,
                &expected_device_id,
            )?),
            ChatLayout { input } => encode(&self.chat_layout_request(&input)?),
            _ => Err(invalid("Invalid chat preference command.")),
        }
    }
}
