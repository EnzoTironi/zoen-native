import assert from 'node:assert/strict';
import { test } from 'node:test';
import { MediaDraftChannel, MediaDraftFailure } from '../media/media-draft-channel.ts';
import type { MediaDraftDto, MediaDraftPort, MediaDraftScope } from '../media/media-draft-types.ts';
import type { TimelineEntry } from './types.ts';

const scope: MediaDraftScope = {
  expected_identity_id: 'owner',
  expected_device_id: 'device',
  space_id: 'chat',
  thread_root_id: 'thread',
};
const original = (): MediaDraftDto => ({
  revision: 'original',
  attempted: false,
  content_json: JSON.stringify({
    kind: 'attachments',
    reply_to: 'inline-reply',
    items: [
      { asset_id: 'a', caption: '  Original 🪁\ncaption  ', mutation_id: 'send_a' },
      { asset_id: 'b', caption: 'Video caption', mutation_id: 'send_b' },
    ],
  }),
  assets: [
    { id: 'a', name: 'photo.png', mime: 'image/png', size: 40, sha256: 'a'.repeat(64) },
    { id: 'b', name: 'video.mov', mime: 'video/quicktime', size: 80, sha256: 'b'.repeat(64) },
  ],
});
const edited = (): MediaDraftDto => {
  const value = original(),
    content = JSON.parse(value.content_json);
  content.items[0] = { ...content.items[0], asset_id: 'edited_a', mutation_id: 'send_edited_a' };
  return {
    ...value,
    revision: 'edited',
    content_json: JSON.stringify(content),
    assets: [
      { ...value.assets[0]!, id: 'edited_a', size: 44, sha256: 'c'.repeat(64) },
      value.assets[1]!,
    ],
  };
};
function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
const tick = () => new Promise<void>((done) => setImmediate(done));
const failure = (kind: MediaDraftFailure['kind']) => (error: unknown) =>
  error instanceof MediaDraftFailure && error.kind === kind;
function fixture(value = original()) {
  let canonical = value;
  const native = deferred<MediaDraftDto>();
  const calls: {
    op: string;
    scope?: MediaDraftScope;
    revision?: string;
    asset?: string;
    token?: string;
  }[] = [];
  const port: MediaDraftPort = {
    async load() {
      calls.push({ op: 'load' });
      return canonical;
    },
    async save(_scope, revision, content, assets) {
      calls.push({ op: 'save', revision });
      assert.equal(revision, canonical.revision);
      assert.equal(content, 'null');
      assert.equal(assets.length, 0);
      canonical = { revision: 'cleared', content_json: 'null', assets: [], attempted: false };
      return { accepted_revision: canonical.revision, draft: canonical };
    },
    async materialize() {
      throw new Error('No row URI is borrowed by native edit');
    },
    async release() {},
    async edit(owner, revision, asset, token) {
      calls.push({ op: 'edit', scope: owner, revision, asset, token });
      return native.promise;
    },
    async cancelPreview(token) {
      calls.push({ op: 'cancel', token });
    },
    async send(owner, revision) {
      calls.push({ op: 'send', scope: owner, revision });
      assert.equal(revision, canonical.revision);
      const entries = JSON.parse(canonical.content_json).items.map(
        (item: { mutation_id: string }) =>
          ({ id: `accepted_${item.mutation_id}` }) as TimelineEntry,
      );
      canonical = { revision: 'accepted', content_json: 'null', assets: [], attempted: false };
      return { entries, draft: canonical };
    },
  };
  return {
    channel: new MediaDraftChannel(scope, port),
    calls,
    native,
    port,
    canonical: (next: MediaDraftDto) => {
      canonical = next;
    },
  };
}

test('native review blocks other writers and adopts exactly the canonical returned draft', async () => {
  const f = fixture();
  await f.channel.load();
  const review = f.channel.edit('a', 'presentation');
  assert.equal(f.channel.getSnapshot().reviewing, true);
  assert.equal(f.channel.getSnapshot().frozen, true);
  await assert.rejects(f.channel.save(JSON.parse(original().content_json)), failure('busy'));
  await assert.rejects(f.channel.send(), failure('busy'));
  await assert.rejects(
    f.channel.stage(async () => {}),
    failure('busy'),
  );
  await tick();
  assert.deepEqual(
    f.calls.find((call) => call.op === 'edit'),
    { op: 'edit', scope, revision: 'original', asset: 'a', token: 'presentation' },
  );
  const changed = edited();
  f.canonical(changed);
  f.native.resolve(changed);
  await review;
  assert.deepEqual(f.channel.getSnapshot().value, changed);
  assert.equal(f.channel.getSnapshot().reviewing, false);
  assert.equal(f.channel.getSnapshot().frozen, false);
  const before = JSON.parse(original().content_json),
    after = f.channel.getSnapshot().content;
  if (after?.kind !== 'attachments') assert.fail();
  assert.equal(after.reply_to, before.reply_to);
  assert.deepEqual(
    after.items.map((item) => item.caption),
    before.items.map((item: { caption: string }) => item.caption),
  );
  assert.equal(after.items[1]!.asset_id, 'b');
  assert.equal(
    f.calls.some((call) => call.op === 'send'),
    false,
    'Done never sends',
  );
  await f.channel.send();
  assert.deepEqual(f.calls.at(-1), { op: 'send', scope, revision: 'edited' });
});

test('lost native edit ACK re-reads Rust rather than restoring obsolete original bytes', async () => {
  const f = fixture();
  await f.channel.load();
  const review = f.channel.edit('a', 'presentation');
  await tick();
  const accepted = edited();
  f.canonical(accepted);
  f.native.reject(new Error('Lost native ACK'));
  await assert.rejects(review, /Lost native ACK/);
  assert.deepEqual(f.channel.getSnapshot().value, accepted);
  assert.equal(f.calls.filter((call) => call.op === 'load').length, 2);
  assert.equal(
    f.calls.some((call) => call.op === 'save' || call.op === 'send'),
    false,
  );
});

test('a conflicting newer journal is adopted without replaying stale reviewed content', async () => {
  const f = fixture();
  await f.channel.load();
  const review = f.channel.edit('a', 'presentation');
  await tick();
  const newer = { ...original(), revision: 'independent-newer' };
  f.canonical(newer);
  f.native.reject(new Error('CAS conflict'));
  await assert.rejects(review, /CAS conflict/);
  assert.deepEqual(f.channel.getSnapshot().value, newer);
  assert.equal(
    f.calls.some((call) => call.op === 'save'),
    false,
  );
});

test('explicit discard cancels native token and waits for dismissal before clearing Rust', async () => {
  const f = fixture();
  await f.channel.load();
  const review = f.channel.edit('a', 'presentation');
  await tick();
  const clear = f.channel.clear();
  await tick();
  assert.deepEqual(f.calls.at(-1), { op: 'cancel', token: 'presentation' });
  assert.equal(
    f.calls.some((call) => call.op === 'save'),
    false,
  );
  f.native.resolve(original());
  await review;
  await clear;
  assert.equal(f.channel.getSnapshot().content, null);
  assert.equal(f.calls.filter((call) => call.op === 'save').length, 1);
  assert.equal(
    f.calls.some((call) => call.op === 'send'),
    false,
  );
});

for (const discard of [false, true]) {
  test(`session retirement (${discard ? 'discard' : 'preserve'}) cancels native edit before draining`, async () => {
    const f = fixture();
    await f.channel.load();
    const review = f.channel.edit('a', 'presentation');
    void review.catch(() => {});
    await tick();
    const retirement = f.channel.retire(discard);
    await tick();
    assert.deepEqual(f.calls.at(-1), { op: 'cancel', token: 'presentation' });
    f.native.resolve(original());
    await review.catch(() => {});
    await retirement;
    assert.equal(f.channel.getSnapshot().phase, 'closed');
    assert.equal(f.channel.getSnapshot().value, null);
    assert.equal(
      f.calls.some((call) => call.op === 'save' || call.op === 'send'),
      false,
    );
  });
}

test('attempted send stays immutable even when an editor would generate fresh IDs', async () => {
  const f = fixture({ ...original(), attempted: true });
  await f.channel.load();
  await assert.rejects(f.channel.edit('a', 'presentation'), failure('busy'));
  assert.equal(
    f.calls.some((call) => call.op === 'edit'),
    false,
  );
});

test('an already admitted preparation stage blocks native review until its save settles', async () => {
  const f = fixture();
  await f.channel.load();
  const gate = deferred<void>();
  f.port.save = async (_owner, expected, json) => {
    assert.equal(expected, 'original');
    const next = { ...original(), revision: 'staged', content_json: json };
    f.canonical(next);
    return { accepted_revision: next.revision, draft: next };
  };
  const stage = f.channel.stage(async (save) => {
    await gate.promise;
    await save(JSON.parse(original().content_json));
  });
  await assert.rejects(f.channel.edit('a', 'presentation'), failure('busy'));
  assert.equal(
    f.calls.some((call) => call.op === 'edit'),
    false,
  );
  gate.resolve();
  await stage;
  assert.equal(f.channel.getSnapshot().value?.revision, 'staged');
});
