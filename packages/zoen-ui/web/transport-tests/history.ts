// Original whole test callbacks, registered in original order by transport.test.ts.
import type { TestContext } from 'node:test';
import {
  install,
  messagingCore,
  linked,
  vault,
  authenticate,
  settle,
  login,
  conversation,
} from './fixture.ts';
import { type CoreOperation } from '../worker-client.ts';
import { Messages } from '../messages.ts';
import assert from 'node:assert/strict';

export const scenario28 = async (t: TestContext) => {
  install(t);
  const base = messagingCore();
  let starts = 0;
  const core = {
    ...base,
    text: async (operation: CoreOperation) => {
      if (operation.kind === 'history_status')
        return JSON.stringify({
          initial_pending: true,
          manifest_received: true,
          chunks_received: 1,
          chunks_total: 3,
          receive_active: false,
        });
      if (operation.kind === 'history_start') {
        starts++;
        return JSON.stringify({
          update: { snapshot: 'history pending', frames: [], conversations: [] },
          work: { kind: 'fetch_manifest', box_id: 'aa'.repeat(32) },
        });
      }
      return base.text(operation);
    },
  };
  const session = {
    ...linked,
    account: { ...linked.account, link_secret: 'ab'.repeat(32), hpke_secret: 'cd'.repeat(32) },
  };
  const messages = new Messages({
    core,
    session,
    vault: { ...vault, save: async () => {} },
    changed: () => {},
  });
  t.after(() => messages.stop());
  await messages.start();
  await authenticate();
  t.mock.timers.tick(1);
  await settle();
  assert.equal(starts, 1);
  assert.deepEqual(await messages.historyStatus(), {
    initial_pending: true,
    manifest_received: true,
    chunks_received: 1,
    chunks_total: 3,
    receive_active: true,
  });
  const one = messages.receiveHistory(30_000).catch(() => null);
  const two = messages.receiveHistory(30_000).catch(() => null);
  t.mock.timers.tick(1000);
  await settle();
  assert.equal(starts, 1);
  messages.stop();
  await Promise.all([one, two]);
};

export const scenario29 = async (t: TestContext) => {
  install(t);
  const codec = messagingCore();
  let retained = false,
    uploaded = false;
  const sealed = [4, 5, 6, ...Array.from({ length: 16 }, () => 7)];
  const work = () =>
    uploaded
      ? { kind: 'complete', messages: 2 }
      : retained
        ? {
            kind: 'upload',
            transfer: 'ab'.repeat(32),
            index: 0,
            sha256: 'cd'.repeat(32),
            bytes: sealed,
            device: login.device,
            timestamp: '123',
            signature: 'ef'.repeat(64),
          }
        : { kind: 'idle' };
  const core = {
    ...codec,
    text: async (op: CoreOperation) => {
      if (op.kind !== 'link_request') return codec.text(op);
      const request = JSON.parse(op.request);
      if (request.op === 'history_send') retained = true;
      if (request.op === 'history_uploaded') {
        assert.equal(request.transfer, 'ab'.repeat(32));
        assert.equal(request.sha256, 'cd'.repeat(32));
        assert.equal(request.index, 0);
        uploaded = true;
      }
      return JSON.stringify({
        result: work(),
        update: retained
          ? {
              snapshot: uploaded ? 'history-ack' : 'history-prepared',
              frames: [],
              conversations: [conversation],
            }
          : null,
        downloads: [],
        uploads: [],
      });
    },
  };
  const events: string[] = [];
  let release = () => {};
  const saved = new Promise<void>((resolve) => {
    release = resolve;
  });
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (session) => {
        if (session.kind !== 'linked') return;
        events.push(session.snapshot);
        if (session.snapshot === 'history-prepared') await saved;
      },
    },
    changed: () => {},
  });
  t.after(() => messages.stop());
  const fetching = t.mock.method(globalThis, 'fetch', async (url, options) => {
    events.push('PUT');
    assert.equal(String(url), 'https://relay.example/v1/transfer/' + 'ab'.repeat(32) + '/0');
    assert.equal(options?.method, 'PUT');
    assert.deepEqual(Array.from(options?.body as Uint8Array), sealed);
    assert.deepEqual(options?.headers, {
      'x-zoen-device': login.device,
      'x-zoen-ts': '123',
      'x-zoen-sig': 'ef'.repeat(64),
    });
    return new Response(null, { status: 200 });
  });
  await messages.start();
  await authenticate();
  const sending = messages.sendHistory('11'.repeat(32));
  await settle();
  assert.equal(fetching.mock.callCount(), 0);
  release();
  assert.equal(await sending, 2);
  assert.equal(fetching.mock.callCount(), 1);
  assert.ok(events.indexOf('history-prepared') < events.indexOf('PUT'));
  assert.ok(events.indexOf('history-ack') > events.indexOf('PUT'));
};

export const scenario30 = async (t: TestContext) => {
  install(t);
  const codec = messagingCore();
  const core = {
    ...codec,
    text: async (op: CoreOperation) => {
      if (op.kind !== 'link_request' || JSON.parse(op.request).op !== 'history_send')
        return codec.text(op);
      return JSON.stringify({
        result: { kind: 'waiting' },
        update: { snapshot: 'history-prepared', frames: [], conversations: [conversation] },
        downloads: [],
        uploads: [],
      });
    },
  };
  const messages = new Messages({
    core,
    session: linked,
    vault: {
      ...vault,
      save: async (session) => {
        if (session.kind === 'linked' && session.snapshot === 'history-prepared')
          throw new Error('QuotaExceededError');
      },
    },
    changed: () => {},
  });
  t.after(() => messages.stop());
  const fetching = t.mock.method(
    globalThis,
    'fetch',
    async () => new Response(null, { status: 200 }),
  );
  await messages.start();
  await authenticate();
  await assert.rejects(messages.sendHistory('11'.repeat(32)), /QuotaExceededError/);
  assert.equal(fetching.mock.callCount(), 0);
  assert.equal(codec.closed, true);
};
