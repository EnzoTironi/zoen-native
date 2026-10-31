import { boundedText, invalid, target } from './media-draft-contract.ts';
import type { MediaDraftChannel } from './media-draft-channel';
import { copyMediaReply, type MediaReplyTarget } from './media-reply.ts';

export type ReviewedAttachmentStage = {
  media: MediaDraftChannel;
  current(): boolean;
  replyTo(): string | null;
  reviewedReply?: () => MediaReplyTarget;
  prepare(text: string): Promise<{ uri: string; release(): Promise<void> }>;
  nextId(): string;
  admitted(): void;
  caption?: string;
};
/** One explicit reviewed file, through the existing journal/CAS and deferred cleanup. */
export async function stageReviewedAttachment({
  media,
  current,
  replyTo,
  reviewedReply,
  prepare,
  nextId,
  admitted,
  caption = '',
  text,
  name,
  mime,
}: ReviewedAttachmentStage & { text: string; name: string; mime: string }): Promise<void> {
  if (!boundedText(caption, 16 * 1024)) invalid();
  if (!current()) throw new Error('retired');
  const captured = reviewedReply?.() ?? { reply_to: replyTo() };
  const reply = Object.freeze({ reply_to: captured.reply_to, ...copyMediaReply(captured) });
  if (!target(reply.reply_to)) invalid();
  await media.load();
  if (!current()) throw new Error('retired');
  const before = media.getSnapshot();
  if (
    before.content ||
    before.frozen ||
    before.saving ||
    before.sending ||
    before.phase !== 'ready'
  )
    throw new Error('media-busy');
  const file = await prepare(text);
  const now = media.getSnapshot();
  if (
    !current() ||
    now.value !== before.value ||
    now.content ||
    now.frozen ||
    now.sending ||
    now.saving
  ) {
    await file.release();
    throw new Error('retired');
  }
  const id = nextId();
  admitted();
  media.deferRelease(file.release);
  await media.save(
    {
      kind: 'attachments',
      ...reply,
      options: { presentation: 'file', grouping: 'separate' },
      items: [{ asset_id: id, mutation_id: id, caption }],
    },
    [{ id, uri: file.uri, name, mime }],
  );
}
