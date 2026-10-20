import test from 'node:test';
import assert from 'node:assert/strict';
import { App } from '@modelcontextprotocol/ext-apps';
import type { JSONRPCMessage, Transport } from '@modelcontextprotocol/client';
import { createDeckcraftSession } from './deckcraft-session.ts';
import type { VerifiedMiniappResource } from './sandbox.ts';
import type { CoreDriver } from '../core/types';
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
function gate() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
async function fixture() {
  let active = true;
  let disposals = 0;
  let transport: Transport;
  const states: { opened: boolean; blocked: boolean; error: string | null }[] = [];
  const calls: string[] = [];
  const shares: unknown[] = [];
  let holdShare: Promise<void> = Promise.resolve();
  const app = new App(
    { name: 'owned-deckcraft-fixture', version: '1' },
    { tools: {} },
    { autoResize: false },
  );
  app.onteardown = async () => {
    calls.push('flush');
    return {};
  };
  app.oncalltool = async ({ name }) => {
    calls.push(name);
    return { content: [] };
  };
  const session = createDeckcraftSession({
    itemId: 'deck-one',
    title: () => 'Owned presentation',
    verified: {
      resource: {
        uri: 'ui://zoen/deckcraft/index.html',
        mime_type: 'text/html;profile=mcp-app',
        text: '<p>Owned</p>',
      },
    } as VerifiedMiniappResource,
    driver: {
      request: (async (command) => {
        calls.push(command.op);
        assert.equal(command.op, 'app_binary_read');
        return { item_id: 'deck-one', version: 7, sha256: 'a'.repeat(64) };
      }) as CoreDriver['request'],
    },
    mutate: async () => {
      throw Error('Unexpected mutation');
    },
    check: () => {
      if (!active) throw Error('Owner retired');
    },
    id: () => 'owned-id',
    context: () => ({ theme: 'dark', displayMode: 'fullscreen' }),
    send: (message) => transport?.onmessage?.(message as JSONRPCMessage),
    disposeFrame: () => {
      disposals++;
    },
    close: () => {},
    onState: (state) => states.push(state),
    share: async (format, head) => {
      shares.push({ format, head });
      await holdShare;
    },
  });
  transport = {
    start: async () => {},
    close: async () => {},
    send: async (message) => {
      void session.receive(JSON.stringify(message));
    },
  };
  await app.connect(transport);
  await tick();
  return {
    app,
    session,
    states,
    calls,
    shares,
    open: () =>
      app.updateModelContext({
        structuredContent: {
          zoen: { binarySave: { itemId: 'deck-one', phase: 'saved', ready: true } },
        },
      }),
    retireOwner: () => {
      active = false;
    },
    holdShare: (pending: Promise<void>) => {
      holdShare = pending;
    },
    disposals: () => disposals,
    close: async () => {
      session.retire();
      await app.close();
    },
  };
}
test('unopened close never waits for an engine callback and stays covered if readiness arrives late', async (t) => {
  const f = await fixture();
  t.after(f.close);
  f.app.onteardown = async () => {
    throw Error('An unfinished engine must not gate Close');
  };
  await f.session.controller.flush({ retire: true });
  await f.open();
  assert.equal(f.states.at(-1)?.blocked, true);
});
test('real SDK teardown ACK gates opened final close and never resumes after its accepted save', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await f.open();
  const held = gate();
  let finished = false;
  f.app.onteardown = async () => {
    f.calls.push('held-flush');
    await held.promise;
    return {};
  };
  const closing = f.session.controller.flush({ retire: true }).then(() => {
    finished = true;
  });
  await tick();
  assert.equal(f.states.at(-1)?.blocked, true);
  assert.equal(finished, false);
  held.resolve();
  await closing;
  assert.equal(f.calls.includes('resume_document'), false);
  assert.equal(finished, true);
});
test('failed ACK preserves the mounted session and allows explicit successful retry', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await f.open();
  f.app.onteardown = async () => {
    throw Error('Synthetic denied save');
  };
  await assert.rejects(f.session.controller.flush({ retire: true }), /Synthetic denied save/);
  assert.equal(f.disposals(), 0);
  assert.equal(f.states.at(-1)?.blocked, false);
  assert.ok(f.states.at(-1)?.error);
  f.app.onteardown = async () => ({});
  await f.session.controller.flush();
  assert.equal(f.states.at(-1)?.error, null);
  assert.ok(f.calls.includes('resume_document'));
});
test('background ACK followed by admitted Close stays serialized and never reopens input', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await f.open();
  const held = gate();
  let flushes = 0;
  f.app.onteardown = async () => {
    if (++flushes === 1) await held.promise;
    return {};
  };
  const background = f.session.controller.flush();
  await tick();
  const close = f.session.controller.flush({ retire: true });
  held.resolve();
  await Promise.all([background, close]);
  assert.equal(flushes, 2);
  assert.equal(f.calls.includes('resume_document'), false);
});
test('owner retirement after ACK prevents resume and suppresses retired state publication', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await f.open();
  const held = gate();
  f.app.onteardown = async () => {
    await held.promise;
    return {};
  };
  const saving = f.session.controller.flush();
  await tick();
  f.retireOwner();
  held.resolve();
  await assert.rejects(saving, /Owner retired/);
  assert.equal(f.calls.includes('resume_document'), false);
  f.session.retire();
  const count = f.states.length;
  await assert.rejects(f.session.controller.flush(), /closed/);
  assert.equal(f.states.length, count);
  assert.equal(f.disposals(), 1);
});
test('share selects exact accepted current version after flush and Close drains share without resume', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await f.open();
  const held = gate();
  f.holdShare(held.promise);
  const sharing = f.session.share('pdf');
  await tick();
  assert.deepEqual(f.shares, [{ format: 'pdf', head: { version: '7', sha256: 'a'.repeat(64) } }]);
  assert.equal(f.states.at(-1)?.blocked, true);
  const closing = f.session.controller.flush({ retire: true });
  let drained = false;
  const drain = f.session.drain().then(() => {
    drained = true;
  });
  await tick();
  assert.equal(drained, false);
  held.resolve();
  await Promise.all([sharing, closing, drain]);
  assert.equal(f.calls.includes('resume_document'), false);
});
test('resource scope never grants generic calls or a different document', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await assert.rejects(f.app.callServerTool({ name: 'app_call_tool', arguments: { id: 'other' } }));
  await assert.rejects(
    f.app.callServerTool({ name: 'app_binary_read', arguments: { id: 'other' } }),
  );
  assert.deepEqual(f.calls, []);
});
