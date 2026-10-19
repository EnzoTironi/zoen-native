import assert from 'node:assert/strict';
import test from 'node:test';
import { parseConversation } from '../../../../packages/zoen-ui/web/messages.ts';
import { browserMessageEntry } from './browser-message-entry.ts';
import { projectConversationTimeline } from './conversation-timeline.ts';
import { searchBrowserMessages } from './browser-search.ts';
import type { ItemDetail, TimelineEntry } from './types';
import type { Persona } from './generated/Persona';

const author = '11'.repeat(32);
const persona = { id: author, name: 'Enzo', is_me: true } as Persona;
const item = {
  id: 'drawing',
  space_id: 'marina',
  kind_id: 'app',
  version: 23,
  app: { app_id: 'excalidraw' },
} as ItemDetail;
const message = (id: string, index: number, activity: boolean) => ({
  id,
  entry: `hash-${id}`,
  seq: String(9007199254741000n + BigInt(index)),
  author,
  name: 'Enzo',
  text: 'Updated the drawing',
  at_ms: index * 1000,
  delivery: 'confirmed',
  failure: null,
  item_edit: activity
    ? {
        space_id: 'marina',
        item_id: 'drawing',
        title: 'Adventure',
        version: index + 2,
        note: 'Updated the drawing',
        is_undo: false,
      }
    : null,
});
const chat = parseConversation({
  id: 'marina',
  title: 'Marina',
  kind: 'direct',
  privacy: 'EndToEnd',
  roster: { [author]: 'Owner' },
  profiles: [],
  prefs: { pinned: false, archived: false, muted_until_ms: null },
  event_count: '30',
  can_send: true,
  waiting: null,
  unreadable: 0,
  messages: [
    ...Array.from({ length: 22 }, (_, i) => message(`edit-${i}`, i, true)),
    message('human-same-words', 23, false),
  ],
});
function entries(): TimelineEntry[] {
  return chat.messages.map((message) => ({
    id: message.id,
    seq: message.seq!,
    at_ms: String(message.at_ms),
    author: persona,
    ...browserMessageEntry(message),
    delivery: 'Sent',
    reply_to: null,
    in_thread: null,
    thread_replies: 0,
    edited: false,
    deleted: false,
    forwarded: false,
    reactions: [],
  }));
}

test('typed browser boundary feeds the shared timeline while every canonical search ID remains reachable', () => {
  const canonical = entries();
  const rows = projectConversationTimeline(canonical, [item], 'marina');
  assert.equal(rows.length, 2);
  assert.equal(rows[0]?.activity?.entries.length, 22);
  assert.equal(rows[0]?.activity?.showCard, true);
  assert.equal(rows[1]?.activity, undefined, 'Identical human wording stays a human message');
  assert.deepEqual(
    rows.flatMap((row) => row.activity?.entries ?? [row.entry]),
    canonical,
  );
  const results = searchBrowserMessages([chat], 'drawing', 'marina', null, 100, () => persona);
  assert.equal(results.hits.length, 23);
  assert.deepEqual(
    new Set(results.hits.map((hit) => hit.ref_id)),
    new Set(canonical.map((entry) => entry.id)),
  );
  for (const target of canonical.slice(0, 22)) {
    const revealed = projectConversationTimeline(canonical, [item], 'marina', target.id);
    assert.deepEqual(revealed.find((row) => row.entry.id === target.id)?.activity?.entries, [
      target,
    ]);
  }
  assert.equal(canonical[0]?.seq, '9007199254741000');
});

test('a canonical human thread keeps its activity root individually addressable', () => {
  const [source] = chat.messages;
  const root = { ...source!, replies: 1 };
  const entry: TimelineEntry = {
    id: root.id,
    seq: root.seq!,
    at_ms: String(root.at_ms),
    author: persona,
    ...browserMessageEntry(root),
    delivery: 'Sent',
    reply_to: null,
    in_thread: null,
    thread_replies: root.replies,
    edited: false,
    deleted: false,
    forwarded: false,
    reactions: [],
  };
  const human = {
    ...entry,
    id: 'human-thread-reply',
    seq: String(BigInt(entry.seq) + 1n),
    kind: { Message: { text: 'Please keep the blue arrow.', card: null } },
    in_thread: root.id,
    thread_replies: 0,
  };
  const rows = projectConversationTimeline([entry, human], [item], 'marina', root.id);
  assert.equal(rows.length, 2);
  assert.strictEqual(rows[0]?.entry, entry);
  assert.equal(
    rows[0]?.activity,
    undefined,
    'The existing message renderer retains its real thread control',
  );
  assert.strictEqual(rows[1]?.entry, human);
  assert.equal(rows[1]?.activity, undefined);
});

test('the real browser snapshot merges projection diagnostics with every existing saved warning', async () => {
  const { build } = await import('esbuild');
  const compiled = await build({
    entryPoints: [new URL('./driver.web.ts', import.meta.url).pathname],
    bundle: true,
    platform: 'node',
    format: 'esm',
    write: false,
  });
  const { createDriver } = await import(
    `data:text/javascript;base64,${Buffer.from(compiled.outputFiles[0]!.text).toString('base64')}`
  );
  const driver = createDriver();
  // A Node-only host contract fixture, never a seeded/authenticated browser UI.
  driver.session = {
    kind: 'linked',
    account: {
      identity: author,
      device: '22'.repeat(32),
      name: 'Enzo',
      handle: 'enzo',
      relay: 'http://127.0.0.1:1',
    },
  };
  driver.state = {
    kind: 'active',
    conversations: [{ ...chat, integrity_error: 'Global signed Item ambiguity' }],
    connection: { kind: 'ready' },
  };
  const saved = {
    marina: 'Existing saved fork takes precedence',
    personal: 'Existing personal-log fork',
    other: 'Other saved warning',
  };
  let warningReads = 0;
  driver.messages = {
    agentRequest: async () => [],
    profileRequest: async () => ({ name: 'Enzo', bio: '' }),
    integrityRequest: async () => {
      warningReads++;
      return saved;
    },
  };
  assert.deepEqual(driver.snapshot().integrity_warnings, {
    marina: 'Global signed Item ambiguity',
  });
  assert.deepEqual((await driver.request({ op: 'snapshot' })).integrity_warnings, saved);
  assert.equal(warningReads, 1, 'The existing saved diagnostics read is retained');
  driver.messages.integrityRequest = async () => ({ personal: saved.personal });
  assert.deepEqual((await driver.request({ op: 'snapshot' })).integrity_warnings, {
    marina: 'Global signed Item ambiguity',
    personal: saved.personal,
  });
});
