import assert from 'node:assert/strict';
import test from 'node:test';
import { prepareMediaReplacement } from '../media/media-replacement-send.ts';
import {
  NormalMessageDrafts,
  type NormalDraftSnapshot,
  type NormalDraftScope,
} from './normal-message-drafts.ts';
import type { DurableAttachmentRuntime } from '../media/durable-attachment-contract.ts';
import type { MediaDraftAttachments } from '../media/media-draft-types.ts';

async function fixture() {
  const scope: NormalDraftScope = {
    owner: { identityId: 'me', deviceId: 'device', sessionEpoch: 1, mode: 'account' },
    spaceId: 'chat',
    threadRootId: null,
  };
  const initial: NormalDraftSnapshot = {
    revision: 'normal-1',
    text: 'Unsent ordinary text',
    reply_to: 'reply',
    edit: {
      message_id: 'message',
      source_revision: 'signed-revision',
      base_text: 'Old caption',
      text: 'Private caption',
      is_caption: true,
    },
  };
  let stored = initial,
    reads = 0,
    writes = 0,
    receipts = 0;
  const controller = new NormalMessageDrafts({
    load: async () => {
      reads++;
      return stored;
    },
    save: async (_scope, revision, text, reply_to, _quote, _shared, edit) => {
      writes++;
      assert.equal(revision, stored.revision);
      return (stored = {
        revision: `normal-${writes + 1}`,
        text,
        reply_to,
        ...(edit ? { edit } : {}),
      });
    },
    send: async () => {
      throw Error('Must not send a normal message');
    },
    sendEdit: async (_scope, revision) => {
      receipts++;
      assert.equal(revision, initial.revision);
      assert.equal(stored.edit, undefined);
      return {
        message_id: 'message',
        text_revision: 'replacement-revision',
        entry: null,
        draft: stored,
      };
    },
  });
  await controller.activate(scope);
  let content: MediaDraftAttachments = {
    kind: 'attachments',
    reply_to: null,
    edit_revision: 'signed-revision',
    items: [{ asset_id: 'replacement', mutation_id: 'intent', caption: 'Replacement caption' }],
  };
  let current = true,
    frozen = false;
  const normal = { controller, scope };
  const context = {
    channel: {
      scope: { space_id: 'chat', thread_root_id: null, edit_message_id: 'message' },
      getSnapshot: () => ({ content, frozen }),
      save: async (next: MediaDraftAttachments) => {
        content = next;
      },
    },
    current: () => current,
    store: { messageDrafts: { channel: () => normal, isCurrent: () => current } },
  } as unknown as Pick<DurableAttachmentRuntime, 'channel' | 'store' | 'current'>;
  return {
    context,
    controller,
    scope,
    content: () => content,
    frozen: () => {
      frozen = true;
    },
    retire: () => {
      current = false;
    },
    accepted: () => {
      stored = { revision: 'completed', text: initial.text, reply_to: initial.reply_to };
    },
    counts: () => ({ reads, writes, receipts }),
  };
}
test('replacement consumes its exact atomic receipt without saving or resending an old caption', async () => {
  const f = await fixture();
  const complete = await prepareMediaReplacement(f.context);
  assert.equal(f.content().normal_revision, 'normal-1');
  f.accepted();
  await complete?.();
  assert.equal(f.controller.getSnapshot(f.scope).edit, null);
  assert.equal(f.controller.getSnapshot(f.scope).text, 'Unsent ordinary text');
  assert.equal(f.controller.getSnapshot(f.scope).replyTo, 'reply');
  assert.deepEqual(f.counts(), { reads: 1, writes: 0, receipts: 1 });
});
test('later typing and a retired account cannot consume an older editor receipt', async () => {
  for (const change of ['typing', 'retire'] as const) {
    const f = await fixture();
    const complete = await prepareMediaReplacement(f.context);
    f.accepted();
    if (change === 'typing') f.controller.setEditText(f.scope, 'New private edit');
    else f.retire();
    await complete?.();
    assert.ok(f.controller.getSnapshot(f.scope).edit);
    assert.equal(f.counts().receipts, 0);
  }
});
test('normal attachments do not activate or flush the message editor', async () => {
  assert.equal(
    await prepareMediaReplacement({ channel: { scope: {} } } as unknown as Pick<
      DurableAttachmentRuntime,
      'channel' | 'store' | 'current'
    >),
    null,
  );
});
