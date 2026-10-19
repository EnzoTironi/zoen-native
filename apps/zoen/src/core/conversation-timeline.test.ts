import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { ItemDetail, TimelineEntry } from './types';
import { projectConversationTimeline } from './conversation-timeline.ts';

function item(patch: Partial<ItemDetail> = {}): ItemDetail {
  return {
    id: 'drawing',
    space_id: 'marina',
    space_title: 'Marina',
    kind_id: 'app',
    kind_label: 'Miniapp',
    title: 'Adventure',
    origin: 'Created',
    created_by: {} as ItemDetail['created_by'],
    plan: null,
    text: null,
    version: 100,
    versions: [],
    linked_requests: [],
    path: '',
    file: null,
    app: {
      app_id: 'excalidraw',
      resource_uri: 'ui://builtin/excalidraw',
      name: 'Excalidraw',
      view_json: '{}',
      headline: '5 elements',
      metrics: [],
      last_action: null,
      trust: 'Act',
      snapshot_json: '',
    },
    ...patch,
  };
}
function update(n: number, patch: Partial<TimelineEntry> = {}): TimelineEntry {
  return {
    id: `edit-${n}`,
    seq: String(9_007_199_254_741_000n + BigInt(n)),
    at_ms: String(n * 1000),
    author: { id: 'me', name: 'Me', is_me: true } as TimelineEntry['author'],
    kind: {
      ItemEdited: {
        item_id: 'drawing',
        title: 'Adventure',
        version: n + 1,
        note: 'Updated the drawing',
        is_undo: false,
      },
    },
    delivery: 'Sent',
    reply_to: null,
    in_thread: null,
    thread_replies: 0,
    edited: false,
    deleted: false,
    forwarded: false,
    reactions: [],
    voice: null,
    ...patch,
  };
}
const message = (id = 'human', text = 'Updated the drawing'): TimelineEntry =>
  update(0, {
    id,
    kind: { Message: { text, card: null } },
  });
const flatten = (rows: ReturnType<typeof projectConversationTimeline>) =>
  rows.flatMap((row) => row.activity?.entries ?? [row.entry]);

test('eleven actual-shaped drawing autosaves become one current card, retaining every canonical event', () => {
  const edits = Array.from({ length: 11 }, (_, index) => update(index));
  const before = JSON.stringify(edits);
  const current = item();
  const rows = projectConversationTimeline(edits, [current], 'marina');
  assert.equal(rows.length, 1);
  assert.equal(rows[0]!.key, edits[0]!.id);
  assert.strictEqual(rows[0]!.entry, edits.at(-1));
  assert.strictEqual(rows[0]!.activity!.item, current);
  assert.deepEqual(flatten(rows), edits);
  rows[0]!.activity!.entries.forEach((entry, index) => assert.strictEqual(entry, edits[index]));
  assert.equal(JSON.stringify(edits), before);
  const grown = projectConversationTimeline([...edits, update(11)], [current], 'marina');
  assert.equal(grown[0]!.key, rows[0]!.key);
});

test('human messages and intentional shares keep exact order even with identical autosave wording', () => {
  const text = message();
  const share = message('share');
  if ('Message' in share.kind)
    share.kind.Message.card = {
      item_id: 'drawing',
      kind_id: 'app',
      app: item().app,
    } as NonNullable<typeof share.kind.Message.card>;
  const entries = [update(0), update(1), text, update(2), share, update(3), update(4)];
  const rows = projectConversationTimeline(entries, [item()], 'marina');
  assert.deepEqual(
    rows.map((row) => row.entry.id),
    ['edit-1', 'human', 'edit-2', 'share', 'edit-4'],
  );
  assert.deepEqual(flatten(rows), entries);
  assert.equal(rows[1]!.activity, undefined);
  assert.equal(rows[3]!.activity, undefined);
  assert.equal(rows[0]!.activity!.showCard, false);
  assert.equal(rows[2]!.activity!.showCard, false);
  assert.equal(rows[4]!.activity!.showCard, true);
});

test('author, thread, undo, note, reaction, reply, receipt and hidden-event boundaries stay separate', () => {
  const first = update(0);
  assert.ok('ItemEdited' in first.kind);
  const base = update(1);
  assert.ok('ItemEdited' in base.kind);
  const edits: Partial<TimelineEntry>[] = [
    { author: { ...first.author, id: 'other' } },
    { in_thread: 'thread-root' },
    { kind: { ItemEdited: { ...base.kind.ItemEdited, is_undo: true } } },
    { kind: { ItemEdited: { ...base.kind.ItemEdited, note: 'Changed permission' } } },
    { kind: { ItemEdited: { ...base.kind.ItemEdited, item_id: 'other-app' } } },
    { reactions: [{ emoji: '❤️', people: [first.author], mine: true }] },
    { reply_to: { id: 'quote', text: 'Reply', author: first.author } },
    { thread_replies: 1 },
    { deleted: true },
    { forwarded: true },
    { delivery: 'Sending' },
    { delivery: 'Failed' },
    { delivery: 'Read' },
    { seq: String(BigInt(base.seq) + 1n) },
    { kind: { ItemEdited: { ...base.kind.ItemEdited, version: 9 } } },
    { at_ms: '180000' },
    { at_ms: '-1' },
    { at_ms: 'NaN' },
  ];
  for (const patch of edits) {
    const second = update(1, patch);
    const entries = [first, second];
    const rows = projectConversationTimeline(
      entries,
      [item(), item({ id: 'other-app' })],
      'marina',
    );
    assert.equal(rows.length, 2, JSON.stringify(patch));
    assert.deepEqual(flatten(rows), entries);
    if (
      second.deleted ||
      second.forwarded ||
      second.reply_to ||
      second.reactions.length ||
      second.thread_replies
    )
      assert.equal(rows[1]!.activity, undefined, 'Annotations keep the existing message controls');
    if (!('at_ms' in patch))
      assert.equal(
        projectConversationTimeline([second, update(2)], [item()], 'marina').length,
        2,
        `A boundary also cannot absorb the following update: ${JSON.stringify(patch)}`,
      );
  }
});

test('missing, removed, foreign or stale app projections retain notes without invented cards', () => {
  const entries = [update(0), update(1)];
  for (const items of [
    [],
    [item({ space_id: 'foreign' })],
    [item({ kind_id: 'note' })],
    [item({ app: null })],
    [item({ version: 0 })],
  ]) {
    const rows = projectConversationTimeline(entries, items, 'marina');
    assert.equal(rows.length, 2);
    assert.ok(rows.every((row) => row.activity && !row.activity.item));
    assert.deepEqual(flatten(rows), entries);
  }
});

test('exact search/jump target splits its run and remains an addressable canonical row', () => {
  const entries = Array.from({ length: 11 }, (_, index) => update(index));
  for (const target of entries) {
    const rows = projectConversationTimeline(entries, [item()], 'marina', target.id);
    const row = rows.find((row) => row.entry.id === target.id)!;
    assert.strictEqual(row.entry, target);
    assert.deepEqual(row.activity!.entries, [target]);
    assert.deepEqual(flatten(rows), entries);
  }
});

test('sequence adjacency remains exact above Number precision and invalid sequences fail open to individual rows', () => {
  for (const seq of [
    '0',
    '-1',
    'NaN',
    '1.2',
    '18446744073709551616',
    '9999999999999999999999999',
  ]) {
    assert.equal(
      projectConversationTimeline([update(0), update(1, { seq })], [item()], 'marina').length,
      2,
    );
  }
  const current = item({ version: 20_001 });
  const entries = Array.from({ length: 20_000 }, (_, index) => update(index));
  const rows = projectConversationTimeline(entries, [current], 'marina');
  assert.equal(rows.length, 1);
  assert.equal(rows[0]!.activity!.entries.length, 20_000);
});
