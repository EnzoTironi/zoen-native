import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { MediaDraftVoice } from '../media/media-draft-types.ts';
import {
  content,
  deferred,
  empty,
  fixture,
  ingest,
  kind,
  scope,
  tick,
} from './media-draft-channel.fixtures.ts';

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
