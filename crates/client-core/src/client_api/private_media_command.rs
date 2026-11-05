use serde::Deserialize;
#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum PrivateMediaCommand {
    RecoverFailedMediaDraft {
        scope: crate::MediaDraftScope,
        message_id: String,
        expected_revision: String,
        expected_message_revision: String,
    },
    MediaPlaybackRead {
        scope: crate::MediaPlaybackScope,
    },
    MediaPlaybackSave {
        scope: crate::MediaPlaybackScope,
        expected_revision: String,
        position: crate::MediaPlaybackPosition,
    },
}
impl PrivateMediaCommand {
    pub(super) fn execute(self, client: &super::Client) -> Result<String, super::CoreError> {
        let engine = client.lock();
        match self {
            Self::RecoverFailedMediaDraft {
                scope,
                message_id,
                expected_revision,
                expected_message_revision,
            } => super::encode(&engine.recover_failed_media_draft(
                &scope,
                &message_id,
                &expected_revision,
                &expected_message_revision,
            )?),
            Self::MediaPlaybackRead { scope } => {
                super::encode(&engine.media_playback_read(&scope)?)
            }
            Self::MediaPlaybackSave {
                scope,
                expected_revision,
                position,
            } => {
                super::encode(&engine.media_playback_save(&scope, &expected_revision, position)?)
            }
        }
    }
}
