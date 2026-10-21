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
  const states: {
    opened: boolean;
    blocked: boolean;
    error: string | null;
    reason?: string | null;
  }[] = [];
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
  assert.equal(f.states.at(-1)?.blocked, true);
  assert.ok(f.states.at(-1)?.error);
  f.app.onteardown = async () => ({});
  await f.session.controller.flush();
  assert.equal(f.states.at(-1)?.error, null);
  assert.ok(f.calls.includes('resume_document'));
});

test('real SDK feedback carries only fixed categories; ordinary error does not impersonate recovery or clear a failed Close', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await f.open();
  const update = (reason: string) =>
    f.app.updateModelContext({
      structuredContent: {
        zoen: {
          binarySave: {
            itemId: 'deck-one',
            phase: 'error',
            ready: true,
            reason,
          },
        },
      },
    });
  for (const reason of ['autosave_busy', 'autosave_failed', 'layout_failed', 'export_failed']) {
    await update(reason);
    assert.equal(f.states.at(-1)?.blocked, false);
    assert.equal(f.states.at(-1)?.reason, reason);
    assert.ok(f.states.at(-1)?.error);
  }
  const payload = 'private document text https://private.invalid/presentation';
  await update(payload);
  assert.equal(f.states.at(-1)?.reason, null);
  assert.equal(JSON.stringify(f.states).includes(payload), false);
  assert.equal(f.states.at(-1)?.blocked, false);
  f.app.onteardown = async () => {
    throw Error('Actual final-save denial');
  };
  await assert.rejects(f.session.controller.flush({ retire: true }), /Actual final-save denial/);
  const failure = f.states.at(-1)?.error;
  assert.equal(f.states.at(-1)?.blocked, true);
  await update('layout_failed');
  await f.open();
  assert.equal(f.states.at(-1)?.blocked, true);
  assert.equal(f.states.at(-1)?.error, failure);
  assert.equal(f.disposals(), 0);
  f.app.onteardown = async () => ({});
  await f.session.controller.flush();
  assert.equal(f.states.at(-1)?.blocked, false);
  assert.equal(f.states.at(-1)?.error, null);
  assert.equal(f.states.at(-1)?.reason, null);
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

for (const retire of [false, true]) {
  test(`late teardown success remains covered until retry resume ACK (retire=${retire})`, async (t) => {
    const f = await fixture();
    t.after(f.close);
    await f.open();
    t.mock.timers.enable({ apis: ['setTimeout'] });
    const write = gate();
    const late = gate();
    let frozen = false;
    f.app.onteardown = async () => {
      frozen = true;
      await write.promise;
      // The real SDK cancellation signal cannot stop this already-admitted
      // document save. A late notification must not acknowledge native resume.
      await f.open();
      late.resolve();
      return {};
    };
    const saving = f.session.controller.flush({ retire });
    await tick();
    t.mock.timers.tick(20_000);
    await assert.rejects(saving, /Request timed out/);
    const failure = f.states.at(-1)?.error;
    assert.ok(failure);
    assert.equal(f.states.at(-1)?.blocked, true);
    write.resolve();
    await late.promise;
    await tick();
    assert.equal(frozen, true);
    assert.equal(f.states.at(-1)?.error, failure);
    assert.equal(f.states.at(-1)?.blocked, true);

    const resume = gate();
    f.app.onteardown = async () => ({});
    f.app.oncalltool = async ({ name }) => {
      assert.equal(name, 'resume_document');
      f.calls.push(name);
      await resume.promise;
      frozen = false;
      return { content: [] };
    };
    const retry = f.session.controller.flush();
    await tick();
    assert.ok(f.calls.includes('resume_document'));
    await f.open();
    assert.equal(f.states.at(-1)?.error, failure);
    assert.equal(f.states.at(-1)?.blocked, true);
    resume.resolve();
    await retry;
    assert.equal(frozen, false);
    assert.equal(f.states.at(-1)?.error, null);
    assert.equal(f.states.at(-1)?.blocked, false);
  });
}

test('timed-out export preflush preserves recovery and an admitted Close never resumes input', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await f.open();
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const write = gate();
  const late = gate();
  let flushes = 0;
  f.app.onteardown = async () => {
    if (++flushes === 1) {
      await write.promise;
      await f.open();
      late.resolve();
    }
    return {};
  };
  const sharing = f.session.share('pptx');
  await tick();
  t.mock.timers.tick(20_000);
  await assert.rejects(sharing, /Request timed out/);
  const failure = f.states.at(-1)?.error;
  assert.ok(failure);
  assert.equal(f.states.at(-1)?.blocked, true);
  assert.deepEqual(f.shares, []);
  assert.equal(f.calls.includes('app_binary_read'), false);
  write.resolve();
  await late.promise;
  await tick();
  assert.equal(f.states.at(-1)?.error, failure);
  assert.equal(f.states.at(-1)?.blocked, true);
  await f.session.controller.flush({ retire: true });
  assert.equal(flushes, 2);
  assert.equal(f.calls.includes('resume_document'), false);
  assert.equal(f.states.at(-1)?.blocked, true);
});

test('resume timeout cannot be acknowledged by a late saved notification', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await f.open();
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const resume = gate();
  const late = gate();
  f.app.oncalltool = async ({ name }) => {
    assert.equal(name, 'resume_document');
    await resume.promise;
    await f.open();
    late.resolve();
    return { content: [] };
  };
  const saving = f.session.controller.flush();
  await tick();
  t.mock.timers.tick(20_000);
  await assert.rejects(saving, /Request timed out/);
  const failure = f.states.at(-1)?.error;
  assert.ok(failure);
  resume.resolve();
  await late.promise;
  await tick();
  assert.equal(f.states.at(-1)?.error, failure);
  assert.equal(f.states.at(-1)?.blocked, true);
  f.app.oncalltool = async () => ({ content: [] });
  await f.session.controller.flush();
  assert.equal(f.states.at(-1)?.blocked, false);
  assert.equal(f.states.at(-1)?.error, null);
});

test('Close admitted before background timeout stays covered through the late save and its own ACK', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await f.open();
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const write = gate();
  const late = gate();
  const final = gate();
  let flushes = 0;
  f.app.onteardown = async () => {
    if (++flushes === 1) {
      await write.promise;
      await f.open();
      late.resolve();
    } else {
      await late.promise;
      await final.promise;
    }
    return {};
  };
  const background = f.session.controller.flush();
  await tick();
  const closing = f.session.controller.flush({ retire: true });
  t.mock.timers.tick(20_000);
  await assert.rejects(background, /Request timed out/);
  await tick();
  write.resolve();
  await late.promise;
  await tick();
  assert.equal(flushes, 2);
  assert.equal(f.states.at(-1)?.blocked, true);
  assert.ok(f.states.at(-1)?.error);
  assert.equal(f.calls.includes('resume_document'), false);
  final.resolve();
  await closing;
  assert.equal(f.calls.includes('resume_document'), false);
  assert.equal(f.states.at(-1)?.blocked, true);
});

test('upstream export intent crosses actual official App tool transport and waits for native presentation', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await f.open();
  const { installManagedHostUI } = await import('../../../../miniapps/deckcraft-ui/src/host-ui.ts');
  let callback: ((format: string) => boolean) | undefined;
  const errors: unknown[] = [];
  const held = gate();
  f.holdShare(held.promise);
  const ui = installManagedHostUI({
    app: f.app,
    itemId: 'deck-one',
    ready: () => true,
    onError: (error) => errors.push(error),
    engine: {
      zoenHostOnExport: (next) => {
        callback = next;
      },
      zoenHostRequest: async () => ({}),
    },
  });
  t.after(() => ui.dispose());
  assert.equal(callback?.('pdf'), true);
  await tick();
  assert.deepEqual(f.shares, [{ format: 'pdf', head: { version: '7', sha256: 'a'.repeat(64) } }]);
  assert.equal(callback?.('pptx'), false);
  held.resolve();
  await tick();
  await tick();
  assert.deepEqual(errors, []);
  assert.ok(f.calls.includes('flush'));
  assert.ok(f.calls.includes('resume_document'));
});
test('native export tool rejects wrong artifacts, unsupported formats and hidden payload fields', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await f.open();
  for (const args of [
    { id: 'other', format: 'pdf' },
    { id: 'deck-one', format: 'png' },
    { id: 'deck-one', format: 'pdf', url: 'https://example.invalid' },
  ])
    await assert.rejects(f.app.callServerTool({ name: 'app_binary_share', arguments: args }));
  assert.deepEqual(f.shares, []);
  assert.deepEqual(f.calls, []);
});
test('opened getter is sticky across guest error labels for stale initial retry fencing', async (t) => {
  const f = await fixture();
  t.after(f.close);
  assert.equal(f.session.opened, false);
  await f.open();
  assert.equal(f.session.opened, true);
  await f.app.updateModelContext({
    structuredContent: {
      zoen: { binarySave: { itemId: 'deck-one', phase: 'error', ready: false } },
    },
  });
  assert.equal(f.session.opened, true);
});

test('unopened startup failure uses a fixed localized reason and never echoes guest exception data', async (t) => {
  const f = await fixture();
  t.after(f.close);
  await f.app.updateModelContext({
    structuredContent: {
      zoen: {
        binarySave: {
          itemId: 'deck-one',
          phase: 'error',
          ready: false,
          reason: 'graphics_unavailable',
        },
      },
    },
  });
  assert.match(f.states.at(-1)!.error!, /graphics/);
  assert.equal(f.session.opened, false);
  await f.app.updateModelContext({
    structuredContent: {
      zoen: {
        binarySave: {
          itemId: 'deck-one',
          phase: 'error',
          ready: false,
          reason: 'Private document text must not appear',
        },
      },
    },
  });
  assert.doesNotMatch(f.states.at(-1)!.error!, /Private document/);
  assert.equal(f.session.opened, false);
});
