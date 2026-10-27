import assert from 'node:assert/strict';
import { test } from 'node:test';
import { protocolStartupFixture } from './protocol-startup.fixtures.ts';

test('an ordered native batch of initialized then the first scene read preserves protocol readiness', async () => {
  const f = protocolStartupFixture();
  try {
    await f.connect();
    assert.equal(f.session.ready, false, 'emitting initialization is not its host ACK');
    const read = f.guest.callServerTool({ name: 'app_scene_read', arguments: { id: 'same-item' } });
    const checked = read.then(
      (value) => ({ value, error: undefined }),
      (error: unknown) => ({ value: undefined, error }),
    );
    await f.batch();
    assert.equal((await checked).error, undefined);
    assert.equal(f.session.ready, true);
    assert.deepEqual(f.calls, ['app_scene_read']);
  } finally {
    await f.close();
  }
});

test('an early tool cannot borrow readiness from a later initialized packet', async () => {
  const f = protocolStartupFixture();
  try {
    await f.connect();
    const initialization = f.inbound.shift()!;
    const read = f.guest.callServerTool({ name: 'app_scene_read' });
    const rejected = read.catch((error: unknown) => error);
    await f.batch();
    assert.match(String(await rejected), /not initialized/);
    assert.deepEqual(f.calls, []);
    f.inbound.push(initialization);
    await f.batch();
    assert.equal(f.session.ready, true);
    assert.deepEqual(f.calls, [], 'the rejected read is never replayed');
  } finally {
    await f.close();
  }
});

test('malformed initialized notification cannot open an initialization barrier', async () => {
  const f = protocolStartupFixture();
  try {
    await f.connect();
    f.inbound.splice(0);
    await f.session.receive(
      JSON.stringify({ jsonrpc: '2.0', method: 'ui/notifications/initialized', params: [] }),
    );
    assert.equal(f.session.ready, false);
    const rejected = f.guest.callServerTool({ name: 'app_scene_read' }).catch((e: unknown) => e);
    await f.batch();
    assert.match(String(await rejected), /not initialized/);
    assert.deepEqual(f.calls, []);
  } finally {
    await f.close();
  }
});

test('initializing requests retain the sixteen-request bound and duplicate ID fence', async () => {
  const f = protocolStartupFixture();
  try {
    await f.connect();
    const pending = Array.from({ length: 17 }, () =>
      f.guest.callServerTool({ name: 'app_scene_read' }).then(
        () => true,
        () => false,
      ),
    );
    f.inbound.push(f.inbound[1]!); // The same first request must not run twice.
    await f.batch();
    assert.equal((await Promise.all(pending)).filter(Boolean).length, 16);
    assert.equal(f.calls.length, 16);
  } finally {
    await f.close();
  }
});

test('retirement between the valid packet and official event cannot admit queued work', async () => {
  const f = protocolStartupFixture();
  try {
    await f.connect();
    const initialized = f.session.receive(JSON.stringify(f.inbound.shift()!));
    const read = f.guest.callServerTool({ name: 'app_scene_read' }).catch(() => null);
    const queued = f.batch();
    f.session.dispose();
    await f.guest.close();
    await Promise.all([initialized, queued, read]);
    assert.equal(f.session.ready, false);
    assert.deepEqual(f.calls, []);
  } finally {
    await f.close();
  }
});
