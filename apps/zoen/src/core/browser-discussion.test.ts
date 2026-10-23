import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';
import type { TimelineEntry } from './types';

const compiled = build({
  entryPoints: [new URL('./driver.web.ts', import.meta.url).pathname],
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false,
}).then(
  async (result) =>
    import(
      `data:text/javascript;base64,${Buffer.from(result.outputFiles[0]!.text).toString('base64')}`
    ),
);
const command = { op: 'item_discussion', item_id: 'page', space_id: 'chat' } as const;
async function fixture() {
  const { createDriver } = await compiled;
  const driver = createDriver();
  driver.session = {
    kind: 'linked',
    account: { identity: 'alice', device: 'phone', name: 'Alice', handle: 'alice' },
  };
  const root = {
    id: 'original-creation',
    entry: 'canonical-hash',
    seq: '9007199254741001',
    author: 'alice',
    name: 'Alice',
    text: '',
    at_ms: 1,
    delivery: 'confirmed',
    failure: null,
    ticks: 'sent',
    edited: false,
    deleted: false,
    forwarded: false,
    reactions: [],
    quote: null,
    thread: null as string | null,
    replies: 2,
    item_edit: null,
    card: { item_id: 'page', kind_id: 'page', title: 'Shared document' },
  };
  driver.state = {
    kind: 'active',
    connection: { kind: 'ready' },
    conversations: [
      {
        id: 'chat',
        title: 'Shared',
        kind: 'group',
        privacy: 'Closed',
        roster: [{ id: 'alice', role: 'Owner' }],
        profiles: [],
        prefs: { pinned: false, archived: false, muted_until_ms: null },
        unread: 0,
        event_count: '3',
        messages: [root],
      },
    ],
  };
  driver.messages = {
    documentRequest: async (request: unknown) => {
      assert.deepEqual(request, command);
      return { item_id: 'page', space_id: 'chat', root_id: root.id };
    },
  };
  return { driver, root };
}

test('browser discussion uses the exact typed timeline root, counts and 64-bit sequence', async () => {
  const { driver } = await fixture();
  const discussion = await driver.request(command);
  const timeline = await driver.request({
    op: 'timeline',
    space_id: 'chat',
    before: null,
    limit: 80,
  });
  assert.deepEqual(discussion, { item_id: 'page', space_id: 'chat', root: timeline.entries[0] });
  const root = discussion.root as TimelineEntry;
  assert.equal(root.seq, '9007199254741001');
  assert.equal(root.thread_replies, 2);
});

test('browser discussion preserves a deleted original root and returns null when no original exists', async () => {
  const { driver, root } = await fixture();
  root.deleted = true;
  assert.equal((await driver.request(command)).root.deleted, true);
  driver.messages.documentRequest = async () => ({
    item_id: 'page',
    space_id: 'chat',
    root_id: null,
  });
  assert.equal((await driver.request(command)).root, null);
});

test('browser discussion refuses foreign artifacts, foreign chats, missing roots and nested replies', async () => {
  for (const field of ['item_id', 'space_id', 'root_id', 'nested'] as const) {
    const { driver, root } = await fixture();
    if (field === 'nested') root.thread = 'another-root';
    driver.messages.documentRequest = async () => ({
      item_id: 'page',
      space_id: 'chat',
      root_id: root.id,
      ...(field === 'nested' ? {} : { [field]: 'foreign' }),
    });
    await assert.rejects(driver.request(command));
  }
});

test('browser discussion propagates Core authorization failure without inventing a local root', async () => {
  const { driver } = await fixture();
  driver.messages.documentRequest = async () => {
    throw new Error('membership revoked');
  };
  await assert.rejects(driver.request(command), /membership revoked/);
});
