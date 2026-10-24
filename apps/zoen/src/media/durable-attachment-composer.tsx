import type { AttachmentComposerOptions } from './attachment-composer';
import { renderDurableAttachment } from './durable-attachment-view';
import type { MediaDraftChannel } from './media-draft-channel';
import { useDurableAttachmentController } from './use-durable-attachment-controller';

/** Rust owns durable content; this route owns only replaceable preview leases. */
export function useDurableAttachmentComposer(
  options: AttachmentComposerOptions,
  channel: MediaDraftChannel | null,
) {
  const controller = useDurableAttachmentController(options, channel);
  const view = renderDurableAttachment({
    ...controller,
    pick: (kind) => controller.pick(kind, field),
  });
  // Photos journals selections while its native overlay still owns the input.
  // Publishing a caption field now would rebind that anchor and retire Photos.
  // Retain the last committed projection (including an ordinary message field)
  // until the picker resolves after native removal. Durable content stays live.
  const field =
    controller.runtime.picking && controller.pickerAnchor?.channel === channel
      ? controller.pickerAnchor.field
      : view.field;
  return { ...view, field };
}
