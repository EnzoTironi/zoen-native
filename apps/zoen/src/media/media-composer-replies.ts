import type { MediaDraftChannel } from './media-draft-channel';
import type { VoiceCaptureContext } from './media-composer-contract';
import type { Recording } from './media-composer-shared';
import type { AttachmentDraftScope } from './attachment-draft';

/** Resolve only the current journal/voice owner. A retiring account's snapshot
 * can never become the quote shown by another channel. */
export function resolveVoiceReplyScope(
  channel: MediaDraftChannel | null,
  clip: Recording | null,
  captured: VoiceCaptureContext | null,
  recording: boolean,
): AttachmentDraftScope | undefined {
  if (!channel) return undefined;
  const content = channel.getSnapshot().content;
  const voice =
    content?.kind === 'voice'
      ? content
      : clip?.durable?.channel === channel
        ? clip.durable.replyContext
        : null;
  const capture = captured?.channel === channel ? captured : null;
  if (!voice && (!recording || !capture)) return undefined;
  return Object.freeze({
    mode: 'account',
    identityId: channel.scope.expected_identity_id,
    deviceId: channel.scope.expected_device_id,
    spaceId: channel.scope.space_id,
    thread: channel.scope.thread_root_id !== null,
    replyTo: voice
      ? (voice.reply_to ?? channel.scope.thread_root_id)
      : (capture?.replyTo ?? channel.scope.thread_root_id),
    replyQuote: voice ? voice.reply_quote : capture?.replyContext.reply_quote,
    sharedReply: voice ? voice.shared_reply : capture?.replyContext.shared_reply,
    replyIntent: voice ? voice.reply_intent : capture?.replyContext.reply_intent,
  });
}
