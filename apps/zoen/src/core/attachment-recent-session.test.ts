import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  AttachmentRecentSession,
  recentAttachmentId,
  type RecentAttachmentPort,
} from '../media/attachment-recent-session.ts';
import type { RecentAttachment } from './generated/RecentAttachment';
import type { RecentAttachmentPage } from './generated/RecentAttachmentPage';
import type { RecentAttachmentReference } from './generated/RecentAttachmentReference';
import type { MediaDraftScope } from '../media/media-draft-types';
import type { AttachmentAsset } from '../media/attachment-draft';
import { MAX_SELECTED_BYTES } from '../media/types.ts';
type MutablePort = { -readonly [Key in keyof RecentAttachmentPort]: RecentAttachmentPort[Key] };

const scope: MediaDraftScope = {
  expected_identity_id: 'account',
  expected_device_id: 'device',
  space_id: 'destination',
  thread_root_id: null,
};
const row = (id: string, patch: Partial<RecentAttachment> = {}): RecentAttachment => ({
  reference: {
    space_id: 'source',
    message_id: id,
    item_id: `file-${id}`,
    version: 1,
    sha256: 'a'.repeat(64),
  },
  name: `${id}.pdf`,
  mime: 'application/pdf',
  bytes: '10',
  at_ms: '100',
  space_title: 'Source',
  author_title: 'Author',
  ready: true,
  ...patch,
});
const page = (
  items: RecentAttachment[],
  next_cursor: string | null = null,
): RecentAttachmentPage => ({ items, next_cursor });
const file = (source: RecentAttachment): AttachmentAsset => ({
  uri: `file:///owned/${source.reference.message_id}`,
  name: source.name,
  mime: source.mime,
  size: Number(source.bytes),
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
async function tick() {
  for (let i = 0; i < 12; i++) await Promise.resolve();
}
function fixture(initial: RecentAttachment[] = [row('a'), row('b'), row('c')]) {
  let active = true;
  const lists: Array<{
    scope: MediaDraftScope;
    kind: string;
    query: string;
    cursor: string | null;
  }> = [];
  const materialized: Array<{
    scope: MediaDraftScope;
    kind: string;
    reference: RecentAttachmentReference;
  }> = [];
  const previews: RecentAttachmentReference[] = [];
  const released: string[] = [];
  const port: MutablePort = {
    async list(owner, kind, query, cursor) {
      lists.push({ scope: owner, kind, query, cursor });
      return page(initial);
    },
    async materialize(owner, kind, reference) {
      materialized.push({ scope: owner, kind, reference: structuredClone(reference) });
      return file(initial.find((item) => item.reference.message_id === reference.message_id)!);
    },
    async preview(_owner, _kind, reference) {
      previews.push(structuredClone(reference));
    },
    async release(uri) {
      released.push(uri);
    },
  };
  const session = new AttachmentRecentSession();
  const current = () => active;
  return {
    session,
    port,
    lists,
    materialized,
    previews,
    released,
    current,
    activate(remaining = 10, destination = scope, kind: 'file' | 'audio' = 'file') {
      session.activate({ scope: destination, port }, kind, remaining, current);
    },
    invalidate() {
      active = false;
    },
  };
}

test('recent sources enumerate only after explicit activation and snapshot the destination', async () => {
  const f = fixture();
  assert.deepEqual(f.lists, []);
  assert.deepEqual(f.session.getSnapshot().items, []);
  await f.session.load(false);
  assert.deepEqual(f.lists, []);
  const destination = { ...scope };
  f.activate(2, destination, 'audio');
  destination.space_id = 'changed-after-activation';
  await tick();
  assert.deepEqual(f.lists, [{ scope, kind: 'audio', query: '', cursor: null }]);
  assert.equal(f.session.getSnapshot().loading, false);
  assert.equal(f.session.getSnapshot().items.length, 3);
  f.activate(2, scope, 'audio');
  await tick();
  assert.equal(f.lists.length, 1, 'the same admitted category does not enumerate again on render');
});

test('late query and initial-page completions cannot overwrite the current search', async () => {
  const f = fixture();
  const requests: Array<{
    query: string;
    gate: ReturnType<typeof deferred<RecentAttachmentPage>>;
  }> = [];
  f.port.list = async (_scope, _kind, query) => {
    const gate = deferred<RecentAttachmentPage>();
    requests.push({ query, gate });
    return gate.promise;
  };
  f.activate();
  f.session.search(true);
  f.session.query('old');
  f.session.query('new');
  assert.deepEqual(
    requests.map((request) => request.query),
    ['', 'old', 'new'],
  );
  requests[2]!.gate.resolve(page([row('current')]));
  await tick();
  requests[1]!.gate.resolve(page([row('stale-search')]));
  requests[0]!.gate.resolve(page([row('stale-initial')]));
  await tick();
  assert.deepEqual(
    f.session.getSnapshot().items.map((item) => item.name),
    ['current.pdf'],
  );
  assert.equal(f.session.getSnapshot().query, 'new');
  assert.equal(f.session.getSnapshot().loading, false);
});

test('owner replacement rejects an old query completion across account and destination changes', async () => {
  for (const replacement of [
    { ...scope, expected_identity_id: 'other-account' },
    { ...scope, space_id: 'other-chat' },
  ]) {
    const f = fixture();
    const old = deferred<RecentAttachmentPage>(),
      next = deferred<RecentAttachmentPage>();
    let calls = 0;
    f.port.list = async () => (++calls === 1 ? old.promise : next.promise);
    f.activate();
    f.activate(2, replacement);
    next.resolve(page([row('new-owner')]));
    await tick();
    old.resolve(page([row('retired-owner')]));
    await tick();
    assert.deepEqual(
      f.session.getSnapshot().items.map((item) => item.name),
      ['new-owner.pdf'],
    );
    assert.equal(f.session.getSnapshot().loading, false);
  }
});

test('keyset pages append in order without duplicate rows and stale cursors cannot cross a new query', async () => {
  const f = fixture();
  const later = deferred<RecentAttachmentPage>();
  const requested: Array<[string, string | null]> = [];
  f.port.list = async (_scope, _kind, query, cursor) => {
    requested.push([query, cursor]);
    if (query === 'new') return page([row('search-result')]);
    if (cursor === 'page-2') return later.promise;
    if (cursor === 'page-1') return page([row('b'), row('c')], 'page-2');
    return page([row('a'), row('b')], 'page-1');
  };
  f.activate();
  await tick();
  await f.session.load(true);
  assert.deepEqual(
    f.session.getSnapshot().items.map((item) => item.name),
    ['a.pdf', 'b.pdf', 'c.pdf'],
  );
  const pending = f.session.load(true);
  f.session.search(true);
  f.session.query('new');
  await tick();
  later.resolve(page([row('stale-cursor')]));
  await pending;
  assert.deepEqual(requested, [
    ['', null],
    ['', 'page-1'],
    ['', 'page-2'],
    ['new', null],
  ]);
  assert.deepEqual(
    f.session.getSnapshot().items.map((item) => item.name),
    ['search-result.pdf'],
  );
});

test('invalid oversized, duplicate and repeating-cursor pages fail without publishing partial rows', async () => {
  const invalid = [
    page(Array.from({ length: 51 }, (_, i) => row(String(i)))),
    page([row('a'), row('a')]),
    page([row('a')], 'x'.repeat(4097)),
  ];
  for (const response of invalid) {
    const f = fixture();
    f.port.list = async () => response;
    f.activate();
    await tick();
    assert.ok(f.session.getSnapshot().error);
    assert.deepEqual(f.session.getSnapshot().items, []);
    assert.equal(f.session.getSnapshot().loading, false);
  }
  const f = fixture();
  f.port.list = async (_scope, _kind, _query, cursor) => page([row(cursor ? 'b' : 'a')], 'cursor');
  f.activate();
  await tick();
  await f.session.load(true);
  assert.ok(f.session.getSnapshot().error);
  assert.deepEqual(
    f.session.getSnapshot().items.map((item) => item.name),
    ['a.pdf'],
  );
});

test('selection admits known bounded files in tap order within the remaining capacity', async () => {
  const invalid = ['0', '-1', '1.5', 'NaN', String(MAX_SELECTED_BYTES + 1), '9007199254740993'];
  const f = fixture([
    row('a'),
    row('b'),
    ...invalid.map((bytes, i) => row(`invalid-${i}`, { bytes })),
  ]);
  f.activate(1);
  await tick();
  f.session.select('unknown');
  for (let i = 0; i < invalid.length; i++)
    f.session.select(recentAttachmentId(row(`invalid-${i}`)));
  assert.deepEqual(f.session.getSnapshot().selected, []);
  f.session.select(recentAttachmentId(row('b')));
  f.session.select(recentAttachmentId(row('a')));
  assert.deepEqual(
    f.session.getSnapshot().selected.map((item) => item.name),
    ['b.pdf'],
  );
  f.session.select(recentAttachmentId(row('b')));
  f.session.select(recentAttachmentId(row('a')));
  assert.deepEqual(
    f.session.getSnapshot().selected.map((item) => item.name),
    ['a.pdf'],
  );
  await assert.rejects(f.session.materialize(['unknown'], f.current));
  await assert.rejects(
    f.session.materialize([recentAttachmentId(row('a')), recentAttachmentId(row('a'))], f.current),
  );
  assert.deepEqual(f.materialized, []);
});

test('known combined byte size cannot exceed the canonical attachment budget during selection', async () => {
  const bytes = String(MAX_SELECTED_BYTES / 2 + 1);
  const rows = [row('a', { bytes }), row('b', { bytes })];
  const f = fixture(rows);
  f.activate();
  await tick();
  f.session.select(recentAttachmentId(rows[0]!));
  f.session.select(recentAttachmentId(rows[1]!));
  assert.deepEqual(
    f.session.getSnapshot().selected.map((item) => item.name),
    ['a.pdf'],
  );
});

test('an oversized materialization batch is refused before reading any individual source', async () => {
  const bytes = String(MAX_SELECTED_BYTES / 2 + 1);
  const rows = [row('a', { bytes }), row('b', { bytes })];
  const f = fixture(rows);
  f.activate();
  await tick();
  await assert.rejects(f.session.materialize(rows.map(recentAttachmentId), f.current));
  assert.deepEqual(f.materialized, []);
});

test('refresh cannot silently replace the exact selected version and hash', async () => {
  const original = row('a');
  const f = fixture([original]);
  f.activate();
  await tick();
  f.session.select(recentAttachmentId(original));
  const changed = row('a', {
    reference: { ...original.reference, version: 2, sha256: 'b'.repeat(64) },
  });
  f.port.list = async () => page([changed]);
  await f.session.load(false);
  assert.ok(f.session.getSnapshot().error);
  assert.deepEqual(f.session.getSnapshot().selected[0]!.reference, original.reference);
  await f.session.materialize([recentAttachmentId(original)], f.current);
  assert.deepEqual(f.materialized[0]!.reference, original.reference);
});

test('selected source intent is detached from mutable port result objects', async () => {
  const source = row('a');
  const pinned = structuredClone(source.reference);
  const f = fixture([source]);
  f.activate();
  await tick();
  f.session.select(recentAttachmentId(source));
  source.reference.version = 9;
  source.reference.sha256 = 'b'.repeat(64);
  await f.session.materialize([recentAttachmentId(source)], f.current);
  assert.deepEqual(
    f.materialized[0]!.reference,
    pinned,
    'selection pins the observed signed source rather than a borrowed object',
  );
});

test('materialization starts exactly one source at a time and preserves the requested order', async () => {
  const rows = [row('a'), row('b'), row('c')];
  const f = fixture(rows);
  const gates = rows.map(() => deferred<AttachmentAsset>());
  const calls: string[] = [];
  f.port.materialize = async (_scope, _kind, reference) => {
    const index = calls.length;
    calls.push(reference.message_id);
    return gates[index]!.promise;
  };
  f.activate();
  await tick();
  const result = f.session.materialize(
    [rows[2]!, rows[0]!, rows[1]!].map(recentAttachmentId),
    f.current,
  );
  assert.deepEqual(calls, ['c']);
  gates[0]!.resolve(file(rows[2]!));
  await tick();
  assert.deepEqual(calls, ['c', 'a']);
  gates[1]!.resolve(file(rows[0]!));
  await tick();
  assert.deepEqual(calls, ['c', 'a', 'b']);
  gates[2]!.resolve(file(rows[1]!));
  assert.deepEqual(
    (await result).map((value) => value.name),
    ['c.pdf', 'a.pdf', 'b.pdf'],
  );
  assert.deepEqual(f.released, []);
});

test('partial materialization failure waits for owned-copy cleanup and stops before the next source', async () => {
  const rows = [row('a'), row('b'), row('c')],
    f = fixture(rows);
  const cleanup = deferred<void>();
  const calls: string[] = [];
  const failure = new Error('source deleted while fetching');
  f.port.materialize = async (_scope, _kind, reference) => {
    calls.push(reference.message_id);
    if (calls.length === 2) throw failure;
    return file(rows[0]!);
  };
  f.port.release = async (uri) => {
    f.released.push(uri);
    await cleanup.promise;
  };
  f.activate();
  await tick();
  let settled = false;
  const operation = f.session.materialize(rows.map(recentAttachmentId), f.current);
  const rejected = assert
    .rejects(operation, (error) => error === failure)
    .finally(() => {
      settled = true;
    });
  await tick();
  assert.deepEqual(calls, ['a', 'b']);
  assert.deepEqual(f.released, ['file:///owned/a']);
  assert.equal(settled, false);
  cleanup.resolve();
  await rejected;
});

test('account retirement and destination replacement reclaim an in-flight copy before rejection', async () => {
  for (const reason of ['account', 'destination']) {
    const source = row('a'),
      f = fixture([source]),
      gate = deferred<AttachmentAsset>();
    f.port.materialize = async () => gate.promise;
    f.activate();
    await tick();
    const operation = f.session.materialize([recentAttachmentId(source)], f.current);
    const rejected = assert.rejects(operation);
    if (reason === 'account') {
      f.invalidate();
      f.session.retire();
    } else f.activate(1, { ...scope, space_id: 'other-destination' });
    gate.resolve(file(source));
    await rejected;
    assert.deepEqual(f.released, ['file:///owned/a']);
  }
});

test('a materialized copy with changed name, MIME or length is reclaimed', async () => {
  for (const patch of [{ name: 'changed.pdf' }, { mime: 'image/png' }, { size: 11 }]) {
    const source = row('a'),
      f = fixture([source]);
    f.port.materialize = async () => ({ ...file(source), ...patch });
    f.activate();
    await tick();
    await assert.rejects(f.session.materialize([recentAttachmentId(source)], f.current));
    assert.deepEqual(f.released, ['file:///owned/a']);
  }
});

test('search rows have no selection affordance and preview uses only an admitted known source', async () => {
  const source = row('a'),
    f = fixture([source]);
  f.activate();
  await tick();
  f.session.search(true);
  f.session.select(recentAttachmentId(source));
  assert.deepEqual(f.session.getSnapshot().selected, []);
  await f.session.preview('unknown', f.current);
  await f.session.preview(recentAttachmentId(source), () => false);
  assert.deepEqual(f.previews, []);
  await f.session.preview(recentAttachmentId(source), f.current);
  assert.deepEqual(f.previews, [source.reference]);
  f.invalidate();
  await f.session.preview(recentAttachmentId(source), () => true);
  f.session.retire();
  await f.session.preview(recentAttachmentId(source), () => true);
  assert.equal(f.previews.length, 1);
});

test('retiring a recent-source owner aborts its in-flight preview before native completion', async () => {
  const source = row('a');
  const f = fixture([source]);
  const completed = deferred<void>();
  let signal: AbortSignal | undefined;
  f.port.preview = async (_scope, _kind, _reference, ownerSignal) => {
    signal = ownerSignal;
    await completed.promise;
  };
  f.activate();
  await tick();
  const preview = f.session.preview(recentAttachmentId(source), f.current);
  assert.equal(signal?.aborted, false);
  f.session.retire();
  assert.equal(signal?.aborted, true);
  completed.resolve();
  await preview;
  assert.deepEqual(f.session.getSnapshot().items, []);
});
