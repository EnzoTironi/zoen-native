import type { NormalDraftChannel } from '../core/normal-message-draft-service';
import type { MediaDraftChannel } from './media-draft-channel';
import { articleText, contactVCard, type SharedContact } from './attachment-content.ts';

/** A native capture updates the existing canonical text draft; this never sends. */
export async function captureArticleDraft(
  channel: NormalDraftChannel,
  capture: () => Promise<string>,
  current: () => boolean,
): Promise<void> {
  if (!current()) throw new Error('retired');
  const text = articleText(await capture());
  if (!current()) throw new Error('retired');
  channel.controller.setText(channel.scope, text);
  await channel.controller.flush(channel.scope);
  if (!current()) throw new Error('retired');
}
/** Stage exactly one reviewed contact through the same media journal and its deferred cleanup. */
export async function stageContactAttachment({
  media,
  contact,
  current,
  replyTo,
  prepare,
  nextId,
  admitted,
}: {
  media: MediaDraftChannel;
  contact: SharedContact;
  current(): boolean;
  replyTo(): string | null;
  prepare(text: string): Promise<{ uri: string; release(): Promise<void> }>;
  nextId(): string;
  admitted(): void;
}): Promise<void> {
  const text = contactVCard(contact);
  if (!current()) throw new Error('retired');
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
      reply_to: replyTo(),
      options: { presentation: 'file', grouping: 'separate' },
      items: [{ asset_id: id, mutation_id: id, caption: '' }],
    },
    [{ id, uri: file.uri, name: 'contact.vcf', mime: 'text/vcard' }],
  );
}
