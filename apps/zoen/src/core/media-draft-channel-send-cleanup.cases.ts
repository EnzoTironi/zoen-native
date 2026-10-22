import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  content,
  deferred,
  fixture,
  ingest,
  kind,
  scope,
  tick,
} from './media-draft-channel.fixtures.ts';

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
