import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { MediaDraftChannel, MediaDraftFailure } from '../media/media-draft-channel.ts';
import type {
  MediaDraftAttachments,
  MediaDraftContent,
  MediaDraftDto,
  MediaDraftIngest,
  MediaDraftPort,
  MediaDraftScope,
  MediaDraftVoice,
} from '../media/media-draft-types.ts';
import type { TimelineEntry } from './types.ts';

const scope: MediaDraftScope = {
  expected_identity_id: 'owner',
  expected_device_id: 'device',
  space_id: 'chat',
  thread_root_id: null,
};
const empty = (revision = '0'): MediaDraftDto => ({
  revision,
  content_json: 'null',
  assets: [],
  attempted: false,
});
const content = (caption = 'one', asset = 'a'): MediaDraftAttachments => ({
  kind: 'attachments',
  reply_to: null,
  items: [{ asset_id: asset, caption, mutation_id: `post_${asset}` }],
});
const multipleContent = (count = 2): MediaDraftAttachments => ({
  kind: 'attachments',
  reply_to: null,
  items: Array.from({ length: count }, (_, index) => {
    const asset = String.fromCharCode(97 + index);
    return { asset_id: asset, caption: '', mutation_id: `post_${asset}` };
  }),
});
const ingest = (id = 'a'): MediaDraftIngest => ({
  id,
  uri: `file:///${id}`,
  name: `${id}.jpg`,
  mime: 'image/jpeg',
});
function deferred<T = void>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
const kind = (name: MediaDraftFailure['kind']) => (error: unknown) =>
  error instanceof MediaDraftFailure && error.kind === name;
function fixture() {
  let current = empty();
  let revision = 0;
  let copies = 0;
  let lease = 0;
  const sources = new Map([
    ['file:///a', 'bytes-a'],
    ['file:///b', 'bytes-b'],
  ]);
  const receipts = new Map<string, string>();
  const sends = new Map<string, readonly TimelineEntry[]>();
  const calls: {
    op: string;
    revision?: string;
    json?: string;
    assets?: readonly MediaDraftIngest[];
  }[] = [];
  const released: string[] = [];
  const hooks: {
    load?: () => Promise<void>;
    save?: () => Promise<void>;
    saved?: () => Promise<void>;
    normalize?: (json: string) => string;
    send?: () => Promise<void>;
    sent?: () => Promise<void>;
    materialize?: () => Promise<void>;
    release?: () => Promise<void>;
  } = {};
  const port: MediaDraftPort = {
    async load() {
      calls.push({ op: 'load' });
      await hooks.load?.();
      return current;
    },
    async save(_scope, expected, json, assets) {
      calls.push({ op: 'save', revision: expected, json, assets });
      await hooks.save?.();
      const supplied = assets.map((asset) => {
        const bytes = sources.get(asset.uri);
        if (!bytes) throw new Error('Source missing');
        return { ...asset, bytes };
      });
      const fingerprint = JSON.stringify([expected, json, supplied]);
      const accepted = receipts.get(fingerprint);
      if (accepted) return { accepted_revision: accepted, draft: current };
      if (expected !== current.revision) throw new Error('Stale');
      if (current.attempted && json !== 'null') throw new Error('Attempted');
      const parsed = JSON.parse(json) as MediaDraftContent | null;
      const refs =
        parsed === null
          ? []
          : parsed.kind === 'attachments'
            ? parsed.items.map((item) => item.asset_id)
            : [parsed.source_asset_id, ...(parsed.render ? [parsed.render.asset_id] : [])];
      const metadata = refs.map((id) => {
        const input = supplied.find((asset) => asset.id === id);
        if (input)
          return {
            id,
            name: input.name,
            mime: input.mime,
            size: Buffer.byteLength(input.bytes),
            sha256: createHash('sha256').update(input.bytes).digest('hex'),
          };
        const old = current.assets.find((asset) => asset.id === id);
        if (!old) throw new Error('Asset missing');
        return old;
      });
      current = {
        revision: `r${++revision}`,
        content_json: hooks.normalize?.(json) ?? json,
        assets: metadata,
        attempted: false,
      };
      const acceptedRevision = current.revision;
      receipts.set(fingerprint, acceptedRevision);
      await hooks.saved?.();
      return { accepted_revision: acceptedRevision, draft: current };
    },
    async send(_scope, expected) {
      calls.push({ op: 'send', revision: expected });
      await hooks.send?.();
      const prior = sends.get(expected);
      if (prior) return { entries: prior, draft: current };
      if (current.revision !== expected) throw new Error('Stale');
      current = { ...current, attempted: true };
      const entries = [{ id: `post-${++copies}` } as TimelineEntry];
      sends.set(expected, entries);
      current = empty(`r${++revision}`);
      await hooks.sent?.();
      return { entries, draft: current };
    },
    async materialize(_scope, expected, id) {
      calls.push({ op: 'materialize', revision: expected });
      const asset = current.assets.find((entry) => entry.id === id);
      if (!asset || current.revision !== expected) throw new Error('Stale');
      await hooks.materialize?.();
      return { ...asset, uri: `file:///lease-${++lease}` };
    },
    async release(uri) {
      released.push(uri);
      await hooks.release?.();
    },
  };
  return {
    port,
    hooks,
    calls,
    sources,
    released,
    channel: new MediaDraftChannel(scope, port),
    current: () => current,
    set: (value: MediaDraftDto) => {
      current = value;
    },
    copies: () => copies,
  };
}

test('Failed recovery adopts a new journal only after the same empty composer admission and never sends', async () => {
  const f = fixture();
  const requests: unknown[] = [];
  f.port.recoverFailed = async (actual, message, mediaRevision, normalRevision) => {
    requests.push([actual, message, mediaRevision, normalRevision]);
    return (await f.port.save(scope, mediaRevision, JSON.stringify(content()), [ingest()])).draft;
  };
  await f.channel.recoverFailed('failed-message', 'normal-7', () => true);
  assert.deepEqual(requests, [[scope, 'failed-message', '0', 'normal-7']]);
  assert.equal(f.channel.getSnapshot().content?.kind, 'attachments');
  assert.equal(f.channel.getSnapshot().frozen, false);
  assert.equal(
    f.calls.some((call) => call.op === 'send'),
    false,
  );
});

test('Failed recovery excludes parallel media writes and preserves late normal typing until explicit reload', async () => {
  const f = fixture(),
    entered = deferred(),
    finish = deferred();
  let current = true;
  f.port.recoverFailed = async (_scope, _message, revision) => {
    entered.resolve();
    await finish.promise;
    return (await f.port.save(scope, revision, JSON.stringify(content()), [ingest()])).draft;
  };
  const work = f.channel.recoverFailed('failed', '0', () => current);
  await entered.promise;
  assert.equal(f.channel.getSnapshot().frozen, true);
  await assert.rejects(f.channel.save(content('later'), [ingest()]), kind('busy'));
  await assert.rejects(f.channel.clear(), kind('busy'));
  await assert.rejects(f.channel.send(), kind('busy'));
  current = false;
  finish.resolve();
  await assert.rejects(work, kind('conflict'));
  assert.equal(
    f.channel.getSnapshot().content,
    null,
    'a late ACK cannot replace the changed composer',
  );
  await f.channel.retry();
  assert.equal(
    f.channel.getSnapshot().content?.kind,
    'attachments',
    'explicit Retry reloads committed canonical data',
  );
  assert.equal(
    f.calls.some((call) => call.op === 'send'),
    false,
  );
});

test('lost Failed recovery ACK reloads the committed draft without repeating the recovery or sending', async () => {
  const f = fixture();
  let recoveries = 0;
  f.port.recoverFailed = async (_scope, _message, revision) => {
    recoveries++;
    await f.port.save(scope, revision, JSON.stringify(content()), [ingest()]);
    throw new Error('Synthetic recovery ACK loss');
  };
  await assert.rejects(
    f.channel.recoverFailed('failed', '0', () => true),
    /ACK loss/,
  );
  assert.equal(f.channel.getSnapshot().content, null);
  await f.channel.retry();
  assert.equal(f.channel.getSnapshot().content?.kind, 'attachments');
  assert.equal(recoveries, 1);
  assert.equal(
    f.calls.some((call) => call.op === 'send'),
    false,
  );
});

test('retiring Failed recovery cannot adopt a prior account result', async () => {
  const f = fixture(),
    entered = deferred(),
    finish = deferred();
  f.port.recoverFailed = async (_scope, _message, revision) => {
    entered.resolve();
    await finish.promise;
    return (await f.port.save(scope, revision, JSON.stringify(content()), [ingest()])).draft;
  };
  const work = f.channel.recoverFailed('failed', '0', () => true);
  await entered.promise;
  const closed = f.channel.retire(false);
  finish.resolve();
  await assert.rejects(work, kind('closed'));
  await closed;
  assert.equal(f.channel.getSnapshot().phase, 'closed');
  assert.equal(f.channel.getSnapshot().content, null);
  assert.equal(
    f.calls.some((call) => call.op === 'send'),
    false,
  );
});

for (const discard of [false, true]) {
  test(`throwing observers cannot abandon ${discard ? 'discard' : 'preserve'} retirement edit cancellation or file-release retry`, async (t) => {
    const f = fixture();
    await f.channel.load();
    await f.channel.save(content(), [ingest()]);
    const canonical = JSON.stringify(f.current());
    const file = await f.channel.materialize('a');
    const editEntered = deferred();
    const editFinished = deferred();
    const cancelled = deferred();
    const releaseEntered = deferred();
    const releaseFinished = deferred();
    const events: string[] = [];
    f.port.edit = async () => {
      editEntered.resolve();
      await editFinished.promise;
      events.push('edit-finished');
      return f.current();
    };
    f.port.cancelPreview = async (token) => {
      assert.equal(token, 'owned-review');
      events.push('cancel-edit');
      cancelled.resolve();
    };
    let failRelease = true;
    f.hooks.release = async () => {
      events.push('release-attempt');
      releaseEntered.resolve();
      await releaseFinished.promise;
      if (failRelease) throw Error('Owned attachment file busy');
      events.push('released');
    };
    let refuse = false;
    let refusedNotifications = 0;
    const healthyStates: { phase: string; value: unknown }[] = [];
    const offBad = f.channel.subscribe(() => {
      if (!refuse) return;
      refusedNotifications++;
      throw Error('Synthetic attachment control notification failure');
    });
    const offHealthy = f.channel.subscribe(() => {
      const state = f.channel.getSnapshot();
      healthyStates.push({ phase: state.phase, value: state.value });
    });
    t.after(async () => {
      offBad();
      offHealthy();
      failRelease = false;
      editFinished.resolve();
      releaseFinished.resolve();
      await f.channel.retire(discard);
    });
    const edit = f.channel.edit('a', 'owned-review');
    void edit.catch(() => {});
    await editEntered.promise;
    refuse = true;
    let retirement!: Promise<void>;
    assert.doesNotThrow(() => {
      retirement = f.channel.retire(discard);
    });
    let deferredCleanups = 0;
    f.channel.deferRelease(async () => {
      deferredCleanups++;
      await f.channel.release(file.uri);
    });
    await cancelled.promise;
    assert.equal(f.channel.getSnapshot().phase, 'closed');
    assert.equal(f.channel.getSnapshot().frozen, true);
    assert.deepEqual(f.released, [], 'The native review still owns the file');
    await assert.rejects(f.channel.save(content('late'), []), kind('closed'));
    editFinished.resolve();
    await edit.catch(() => {});
    await releaseEntered.promise;
    releaseFinished.resolve();
    await assert.rejects(retirement, /Owned attachment file busy/);
    assert.equal(deferredCleanups, 1);
    assert.deepEqual(f.released, [file.uri]);
    failRelease = false;
    await f.channel.retire(discard);
    assert.equal(deferredCleanups, 2, 'Failed cleanup remains owned and retries');
    assert.deepEqual(events, [
      'cancel-edit',
      'edit-finished',
      'release-attempt',
      'release-attempt',
      'released',
    ]);
    assert.ok(refusedNotifications > 0);
    assert.ok(healthyStates.length > 0);
    assert.deepEqual(healthyStates.at(-1), { phase: 'closed', value: null });
    assert.equal(f.channel.getSnapshot().content, null);
    assert.equal(
      JSON.stringify(f.current()),
      canonical,
      'Retirement does not replay or delete the journal',
    );
    assert.equal(f.calls.filter((call) => call.op === 'save').length, 1);
    assert.equal(
      f.calls.some((call) => call.op === 'send'),
      false,
    );
    await f.channel.retire(discard);
    assert.equal(deferredCleanups, 2);
    assert.deepEqual(
      f.released,
      [file.uri, file.uri],
      'Exactly one failed and one successful release',
    );
  });
}

test('read is explicit/coalesced and malformed or unread drafts never admit overwrite', async () => {
  const f = fixture();
  const gate = deferred();
  f.hooks.load = () => gate.promise;
  assert.equal(f.calls.length, 0);
  await assert.rejects(f.channel.save(content(), [ingest()]), kind('notLoaded'));
  const first = f.channel.load();
  assert.equal(first, f.channel.load());
  gate.resolve();
  await first;
  assert.equal(f.calls.length, 1);
  assert.ok(Object.isFrozen(f.channel.getSnapshot()));
  const broken = fixture();
  broken.set({ ...empty(), content_json: '{broken' });
  await assert.rejects(broken.channel.load());
  await assert.rejects(broken.channel.clear());
  assert.equal(broken.calls.filter((call) => call.op === 'save').length, 0);
});

test('CAS queue keeps optimistic captions and every new ingress across failed writes', async () => {
  const f = fixture();
  await f.channel.load();
  const entered = deferred();
  const gate = deferred();
  let once = true;
  f.hooks.save = async () => {
    if (once) {
      once = false;
      entered.resolve();
      await gate.promise;
      throw new Error('Disk full');
    }
  };
  const a = f.channel.save(content(), [ingest()]);
  const rejected = assert.rejects(a, /Disk full/);
  await entered.promise;
  const next = {
    kind: 'attachments' as const,
    reply_to: null,
    items: [
      ...content().items.map((item) => ({ ...item })),
      { asset_id: 'b', caption: 'second', mutation_id: 'post_b' },
    ],
  };
  const b = f.channel.save(next, [ingest('b')]);
  const rejectedB = assert.rejects(b, /Disk full/);
  assert.equal(f.channel.getSnapshot().value?.revision, '0');
  assert.deepEqual(f.channel.getSnapshot().content, next);
  next.items[1]!.caption = 'outside mutation';
  gate.resolve();
  await Promise.all([rejected, rejectedB]);
  let released = 0;
  f.channel.deferRelease(async () => {
    released++;
  });
  await tick();
  assert.equal(released, 0);
  await f.channel.retry();
  await f.channel.flush();
  assert.equal(JSON.parse(f.current().content_json).items[1].caption, 'second');
  assert.deepEqual(
    f.current().assets.map((asset) => asset.id),
    ['a', 'b'],
  );
  assert.equal(released, 1);
});

test('lost save ACK retries original revision, descriptors/order and bytes; JSON equality is insufficient', async () => {
  const f = fixture();
  await f.channel.load();
  let once = true;
  f.hooks.saved = async () => {
    if (once) {
      once = false;
      throw new Error('Lost ACK');
    }
  };
  await assert.rejects(f.channel.save(content(), [ingest()]), /Lost ACK/);
  f.sources.set('file:///a', 'changed');
  await assert.rejects(f.channel.retry(), /Stale/);
  assert.equal(f.channel.getSnapshot().content?.kind, 'attachments');
  f.sources.set('file:///a', 'bytes-a');
  await f.channel.retry();
  assert.equal(f.current().revision, 'r1');
  const saves = f.calls.filter((call) => call.op === 'save');
  assert.deepEqual(
    saves.map((call) => call.revision),
    ['0', '0', '0'],
  );
  assert.deepEqual(saves[0]!.assets, saves[2]!.assets);
});

test('exact old save replay adopts newer canonical draft without overwriting it', async () => {
  const f = fixture();
  await f.channel.load();
  let once = true;
  f.hooks.saved = async () => {
    if (once) {
      once = false;
      throw new Error('Lost ACK');
    }
  };
  await assert.rejects(f.channel.save(content('old'), [ingest()]), /Lost ACK/);
  await f.port.save(scope, f.current().revision, JSON.stringify(content('newer')), []);
  const newer = f.current();
  await f.channel.retry();
  assert.deepEqual(f.current(), newer);
  const restored = f.channel.getSnapshot().content;
  assert.ok(restored?.kind === 'attachments');
  assert.equal(restored.items[0]!.caption, 'newer');
});

test('accepted native ACK admits real waveform float normalization and adopts canonical editor/render values', async () => {
  const f = fixture();
  await f.channel.load();
  const levels = Array.from({ length: 160 }, (_, index) => Math.fround(0.1251579523 + index / 512));
  assert.equal(levels[0], 0.12515795230865479);
  f.hooks.normalize = (json) =>
    json
      .replaceAll('0.12515795230865479', '0.1251579523086548')
      .replaceAll('0.4000000059604645', '0.4');
  const source = { ...ingest(), name: 'source.m4a', mime: 'audio/mp4' };
  const voice: MediaDraftVoice = {
    kind: 'voice',
    reply_to: null,
    source_asset_id: 'a',
    editor: { original: { id: 'native-original', levels } },
    render: null,
  };
  const saved = await f.channel.save(voice, [source]);
  assert.equal(saved.revision, 'r1');
  assert.notEqual(saved.content_json, f.calls.find((call) => call.op === 'save')!.json);
  assert.equal(JSON.stringify(f.channel.getSnapshot().content), saved.content_json);
  assert.equal(f.channel.getSnapshot().error, null);
  const rendered: MediaDraftVoice = {
    ...voice,
    render: {
      asset_id: 'b',
      mime: 'audio/mp4',
      duration_ms: 1910.5,
      levels: [Math.fround(0.4)],
      transcript: '',
    },
  };
  const final = await f.channel.save(rendered, [
    { ...ingest('b'), name: 'render.m4a', mime: 'audio/mp4' },
  ]);
  const canonical = f.channel.getSnapshot().content;
  assert.ok(canonical?.kind === 'voice');
  assert.deepEqual(canonical.render?.levels, [0.4]);
  assert.equal(final.revision, 'r2');
  assert.equal(f.channel.getSnapshot().error, null);
  await f.channel.send();
  assert.equal(f.copies(), 1);
});

test('old ACK with an identical-JSON newer revision still refuses pending stale edits', async () => {
  const f = fixture();
  await f.channel.load();
  let once = true;
  f.hooks.saved = async () => {
    if (once) {
      once = false;
      throw new Error('Lost ACK');
    }
  };
  await assert.rejects(f.channel.save(content('same'), [ingest()]), /Lost ACK/);
  await assert.rejects(f.channel.save(content('pending stale')), /Lost ACK/);
  await f.port.save(scope, f.current().revision, f.current().content_json, []);
  const newer = f.current();
  assert.equal(newer.revision, 'r2');
  await assert.rejects(f.channel.retry(), kind('conflict'));
  assert.deepEqual(f.current(), newer);
  assert.equal(f.channel.getSnapshot().value?.revision, 'r2');
  await assert.rejects(f.channel.retry(), /Stale/);
  assert.deepEqual(f.current(), newer);
});

test('send freezes synchronously; ambiguous retry uses the receipt and preserves newer canonical data', async () => {
  const f = fixture();
  await f.channel.load();
  await f.channel.save(content(), [ingest()]);
  let once = true;
  f.hooks.sent = async () => {
    if (once) {
      once = false;
      throw new Error('Lost send ACK');
    }
  };
  const sending = f.channel.send();
  assert.equal(f.channel.getSnapshot().frozen, true);
  await assert.rejects(sending, /Lost send ACK/);
  await assert.rejects(f.channel.save(content('edit')), kind('attempted'));
  await f.port.save(scope, f.current().revision, JSON.stringify(content('newer', 'b')), [
    ingest('b'),
  ]);
  const newer = f.current();
  const result = await f.channel.send();
  assert.equal(result.entries[0]?.id, 'post-1');
  assert.deepEqual(result.draft, newer);
  assert.equal(f.copies(), 1);
  assert.equal(f.channel.getSnapshot().frozen, false);
});

test('definitive preflight rejection unfreezes but explicit ambiguous discard never retries send', async () => {
  const f = fixture();
  await f.channel.load();
  await f.channel.save(content(), [ingest()]);
  f.hooks.send = async () => {
    throw Object.assign(new Error('Missing quote'), { code: 'ERR_ZOEN_DRAFT_REJECTED' });
  };
  await assert.rejects(f.channel.send(), /Missing quote/);
  assert.equal(f.channel.getSnapshot().frozen, false);
  f.hooks.send = async () => {
    throw new Error('Unknown acceptance');
  };
  await assert.rejects(f.channel.send());
  assert.equal(f.channel.getSnapshot().frozen, true);
  f.hooks.send = undefined;
  let once = true;
  f.hooks.save = async () => {
    if (once) {
      once = false;
      throw new Error('Clear failed');
    }
  };
  await assert.rejects(f.channel.clear(), /Clear failed/);
  assert.equal(f.channel.getSnapshot().frozen, true);
  await f.channel.clear();
  assert.equal(f.current().content_json, 'null');
  assert.equal(f.channel.getSnapshot().frozen, false);
  assert.equal(f.copies(), 0);
  assert.equal(f.calls.filter((call) => call.op === 'send').length, 2);
});

test('discard waits in-flight send and cannot erase a separately newer canonical draft', async () => {
  const f = fixture();
  await f.channel.load();
  await f.channel.save(content(), [ingest()]);
  const gate = deferred();
  const entered = deferred();
  f.hooks.sent = async () => {
    entered.resolve();
    await gate.promise;
    throw new Error('Lost ACK');
  };
  const sending = f.channel.send();
  const failed = assert.rejects(sending);
  await entered.promise;
  const clearing = f.channel.clear();
  await f.port.save(scope, f.current().revision, JSON.stringify(content('newer', 'b')), [
    ingest('b'),
  ]);
  const newer = f.current();
  gate.resolve();
  await failed;
  assert.deepEqual(await clearing, newer);
  assert.deepEqual(f.current(), newer);
  assert.equal(f.copies(), 1);
});

test('deferred cleanup and lease release coalesce across auto-drain, flush and retirement; failures retain ownership', async () => {
  const f = fixture();
  await f.channel.load();
  await f.channel.save(content(), [ingest()]);
  const materialized = await f.channel.materialize('a');
  const gate = deferred();
  f.hooks.release = () => gate.promise;
  let cleanups = 0;
  f.channel.deferRelease(async () => {
    cleanups++;
    await f.channel.release(materialized.uri);
  });
  const flushing = f.channel.flush();
  const retiring = f.channel.retire(false);
  await tick();
  assert.equal(cleanups, 1);
  assert.equal(f.released.length, 1);
  gate.resolve();
  await Promise.all([flushing, retiring]);
  assert.equal(f.released.length, 1);
  const retry = fixture();
  await retry.channel.load();
  await retry.channel.save(content(), [ingest()]);
  const asset = await retry.channel.materialize('a');
  let once = true;
  retry.hooks.release = async () => {
    if (once) {
      once = false;
      throw new Error('Busy file');
    }
  };
  await assert.rejects(retry.channel.release(asset.uri), /Busy file/);
  await retry.channel.release(asset.uri);
  assert.equal(retry.released.length, 2);
});

test('synchronously admitted prepare saves during preserve retirement, but erase drains and rejects its save', async () => {
  for (const discard of [false, true]) {
    const f = fixture();
    await f.channel.load();
    const gate = deferred();
    const entered = deferred();
    let cleaned = false;
    const stage = f.channel.stage(async (save) => {
      entered.resolve();
      await gate.promise;
      await save(content(), [ingest()]);
    });
    const result = stage.then(
      () => 'saved',
      () => 'cancelled',
    );
    await entered.promise;
    f.channel.deferRelease(async () => {
      cleaned = true;
    });
    const retirement = f.channel.retire(discard);
    await assert.rejects(f.channel.save(content(), [ingest()]), kind('closed'));
    await tick();
    assert.equal(cleaned, false);
    gate.resolve();
    await retirement;
    assert.equal(await result, discard ? 'cancelled' : 'saved');
    assert.equal(f.current().content_json === 'null', discard);
    assert.equal(cleaned, true);
  }
});

test('failed preparation is retained for explicit retry, and old-owner materialization is released', async () => {
  const f = fixture();
  await f.channel.load();
  let once = true;
  await assert.rejects(
    f.channel.stage(async (save) => {
      if (once) {
        once = false;
        throw new Error('Prepare failed');
      }
      await save(content(), [ingest()]);
    }),
    /Prepare failed/,
  );
  await assert.rejects(f.channel.flush(), /Prepare failed/);
  await f.channel.retry();
  assert.notEqual(f.current().content_json, 'null');
  const gate = deferred();
  const entered = deferred();
  f.hooks.materialize = async () => {
    entered.resolve();
    await gate.promise;
  };
  const materialized = f.channel.materialize('a');
  const failed = assert.rejects(materialized, kind('closed'));
  await entered.promise;
  const retirement = f.channel.retire(false);
  gate.resolve();
  await Promise.all([failed, retirement]);
  assert.deepEqual(f.released, ['file:///lease-1']);
});

test('explicit Cancel retires failed and active preparation without retry or a save behind clear', async () => {
  const f = fixture();
  await f.channel.load();
  let preparations = 0;
  await assert.rejects(
    f.channel.stage(async () => {
      preparations++;
      throw new Error('Failed prepare');
    }),
  );
  await f.channel.clear();
  await f.channel.flush();
  await f.channel.retire(false);
  assert.equal(preparations, 1);
  const active = fixture();
  await active.channel.load();
  const gate = deferred();
  const entered = deferred();
  const preparation = active.channel.stage(async (save) => {
    entered.resolve();
    await gate.promise;
    await save(content(), [ingest()]);
  });
  const failed = assert.rejects(preparation, kind('closed'));
  await entered.promise;
  const clearing = active.channel.clear();
  gate.resolve();
  await Promise.all([failed, clearing]);
  await active.channel.flush();
  await active.channel.retire(false);
  assert.deepEqual(
    active.calls.filter((call) => call.op === 'save').map((call) => call.json),
    ['null'],
  );
  assert.equal(active.current().content_json, 'null');
});

test('metadata validates closed unions and rejects native URIs or SDK objects in editor JSON', async () => {
  const f = fixture();
  await f.channel.load();
  const source = { ...ingest(), name: 'a.m4a', mime: 'audio/mp4' };
  const voice = {
    kind: 'voice' as const,
    reply_to: null,
    source_asset_id: 'a',
    editor: { selection: [0, 1] },
    render: null,
  };
  for (const editor of [
    { uri: 'file:///secret' },
    { original: { data: '/private/cache/secret' } },
    new Date(),
    { bad: NaN },
  ])
    await assert.rejects(f.channel.save({ ...voice, editor }, [source]), kind('invalid'));
  await assert.rejects(
    f.channel.save({ ...content(), unknown: true } as unknown as MediaDraftContent, [ingest()]),
    kind('invalid'),
  );
  await f.channel.save(voice, [source]);
  assert.equal(f.current().assets[0]?.mime, 'audio/mp4');
  assert.ok(Object.isFrozen(f.channel.getSnapshot().content));
});

test('attachment defaults preserve legacy content JSON on save and restored native projection', async () => {
  const f = fixture();
  await f.channel.load();
  const legacy = content();
  const explicit = {
    ...legacy,
    options: { presentation: 'media' as const, grouping: 'separate' as const },
  };
  await f.channel.save(explicit, [ingest()]);
  assert.equal(f.calls.find((call) => call.op === 'save')?.json, JSON.stringify(legacy));
  assert.equal(f.current().content_json, JSON.stringify(legacy));
  assert.deepEqual(f.channel.getSnapshot().content, legacy);

  f.set({ ...f.current(), content_json: JSON.stringify(explicit) });
  const restored = new MediaDraftChannel(scope, f.port);
  await restored.load();
  assert.equal(restored.getSnapshot().value?.content_json, JSON.stringify(legacy));
  assert.deepEqual(restored.getSnapshot().content, legacy);
});

test('attachment nondefault presentation and grouping persist in the same canonical content', async () => {
  for (const options of [
    { presentation: 'file', grouping: 'separate' },
    { presentation: 'media', grouping: 'grouped' },
    { presentation: 'file', grouping: 'grouped' },
  ] as const) {
    const f = fixture();
    await f.channel.load();
    const selected = { ...multipleContent(), options };
    await f.channel.save(selected, [ingest(), ingest('b')]);
    assert.equal(f.current().content_json, JSON.stringify(selected));
    const projected = f.channel.getSnapshot().content;
    assert.ok(projected?.kind === 'attachments');
    assert.deepEqual(projected.options, options);
    assert.ok(Object.isFrozen(projected.options));

    const restored = new MediaDraftChannel(scope, f.port);
    await restored.load();
    assert.deepEqual(restored.getSnapshot().content, selected);
  }
});

test('attachment options reject malformed, partial, unsupported and quality fields before any native save', async () => {
  const f = fixture();
  await f.channel.load();
  for (const options of [
    null,
    undefined,
    false,
    [],
    {},
    { presentation: 'file' },
    { grouping: 'grouped' },
    { presentation: 'document', grouping: 'separate' },
    { presentation: 'media', grouping: 'album' },
    { presentation: 'media', grouping: 'separate', quality: 'original' },
  ])
    await assert.rejects(
      f.channel.save({ ...multipleContent(), options } as unknown as MediaDraftContent, [
        ingest(),
        ingest('b'),
      ]),
      kind('invalid'),
    );
  await assert.rejects(
    f.channel.save({ ...multipleContent(), quality: 'original' } as unknown as MediaDraftContent, [
      ingest(),
      ingest('b'),
    ]),
    kind('invalid'),
  );
  assert.equal(f.calls.filter((call) => call.op === 'save').length, 0);
  assert.equal(f.channel.getSnapshot().content, null);
});

test('grouped attachments accept exactly two through ten assets and reject singletons or overflow', async () => {
  const options = { presentation: 'media' as const, grouping: 'grouped' as const };
  for (const count of [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]) {
    const f = fixture();
    await f.channel.load();
    const selected = { ...multipleContent(count), options };
    const inputs = selected.items.map((item) => {
      const input = ingest(item.asset_id);
      f.sources.set(input.uri, `bytes-${input.id}`);
      return input;
    });
    if (count === 1 || count === 11) {
      await assert.rejects(f.channel.save(selected, inputs), kind('invalid'));
      assert.equal(f.calls.filter((call) => call.op === 'save').length, 0);
    } else {
      await f.channel.save(selected, inputs);
      assert.deepEqual(f.channel.getSnapshot().content, selected);
      assert.equal(f.current().assets.length, count);
    }
  }
});

test('ambiguous send retains immutable attachment options and retries its original revision', async () => {
  const f = fixture();
  await f.channel.load();
  const options: {
    presentation: 'media' | 'file';
    grouping: 'separate' | 'grouped';
  } = { presentation: 'file', grouping: 'grouped' };
  const selected = { ...multipleContent(), options };
  await f.channel.save(selected, [ingest(), ingest('b')]);
  const canonical = f.current();
  f.hooks.send = async () => {
    throw new Error('Unknown acceptance');
  };
  await assert.rejects(f.channel.send(), /Unknown acceptance/);
  options.presentation = 'media';
  options.grouping = 'separate';
  assert.equal(f.channel.getSnapshot().frozen, true);
  const projected = f.channel.getSnapshot().content;
  assert.ok(projected?.kind === 'attachments');
  assert.deepEqual(projected.options, { presentation: 'file', grouping: 'grouped' });
  assert.deepEqual(f.current(), canonical);
  await assert.rejects(f.channel.save(selected), kind('attempted'));
  assert.equal(f.calls.filter((call) => call.op === 'save').length, 1);

  f.hooks.send = undefined;
  await f.channel.retry();
  assert.deepEqual(
    f.calls.filter((call) => call.op === 'send').map((call) => call.revision),
    [canonical.revision, canonical.revision],
  );
  assert.equal(f.copies(), 1);
  assert.equal(f.channel.getSnapshot().content, null);
  assert.equal(f.channel.getSnapshot().frozen, false);
});
