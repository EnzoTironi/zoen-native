import type { AttachmentComposerOptions } from './attachment-composer';
import { renderDurableAttachment } from './durable-attachment-view';
import type { MediaDraftChannel } from './media-draft-channel';
import { useDurableAttachmentController } from './use-durable-attachment-controller';

/** Rust owns durable content; this route owns only replaceable preview leases. */
export function useDurableAttachmentComposer(
  options: AttachmentComposerOptions,
  channel: MediaDraftChannel | null,
) {
  return renderDurableAttachment(useDurableAttachmentController(options, channel));
}
