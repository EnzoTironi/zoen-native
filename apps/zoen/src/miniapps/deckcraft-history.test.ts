import test from 'node:test';
import assert from 'node:assert/strict';
import { App } from '@modelcontextprotocol/ext-apps';
import type { JSONRPCMessage, Transport } from '@modelcontextprotocol/client';
import type { CoreDriver } from '../core/types';
import type { VerifiedMiniappResource } from './sandbox';
import { createDeckcraftSession } from './deckcraft-session.ts';
import { attachDeckcraftDocument } from './deckcraft-document.ts';
import { dto, original, changed, enginePort, tick } from './canonical-binary.fixtures.ts';
import { hash } from './canonical-scene.ts';

async function history() {
  let active = true;
  let current = 2;
  let transport: Transport;
  let reads = 0;
  let mutations = 0;
  let shareFailure = false;
  let heldShare: Promise<void> = Promise.resolve();
  const states: { blocked: boolean; reason: string | null }[] = [];
  const shares: unknown[] = [];
  const head = { version: '1', sha256: hash(original) };
  const app = new App({ name: 'Deck history', version: '1' }, { tools: {} }, { autoResize: false });
  const session = createDeckcraftSession({
    itemId: 'it_deck',
    selectedHead: head,
    title: () => 'Historical deck',
    verified: {
      resource: {
        uri: 'ui://zoen/deckcraft/index.html',
        text: '',
        mime_type: 'text/html;profile=mcp-app',
      },
    } as VerifiedMiniappResource,
    driver: {
      request: (async (command) => {
        assert.equal(command.op, 'app_binary_read');
        if (command.op !== 'app_binary_read') throw Error('unexpected command');
        // Even the guest's unqualified open must resolve the historical source, never current.
        assert.equal(command.version, 1);
        assert.equal(command.expected_sha256, head.sha256);
        reads++;
        return { ...dto(original), current_version: current };
      }) as CoreDriver['request'],
    },
    mutate: (async () => {
      mutations++;
      throw Error('No historical writes');
    }) as CoreDriver['request'],
    check: () => {
      if (!active) throw Error('Owner retired');
    },
    id: () => 'transfer-one',
    context: () => ({ theme: 'dark' }),
    send: (message) => transport.onmessage?.(message as JSONRPCMessage),
    disposeFrame: () => {},
    close: () => {},
    onState: (state) => states.push(state),
    share: async (format, selected) => {
      if (shareFailure) throw Error('Native sharing unavailable');
      shares.push({ format, selected });
      await heldShare;
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
  const engine = enginePort();
  const owner = await attachDeckcraftDocument({
    app,
    engine,
    itemId: 'it_deck',
    id: () => 'export-one',
  });
  await app.updateModelContext({
    structuredContent: {
      zoen: {
        binarySave: { itemId: 'it_deck', phase: 'saved', ready: true },
      },
    },
  });
  return {
    app,
    session,
    engine,
    owner,
    head,
    shares,
    states,
    holdShare: (pending: Promise<void>) => {
      heldShare = pending;
    },
    failShare: () => {
      shareFailure = true;
    },
    reads: () => reads,
    mutations: () => mutations,
    advance: () => {
      current++;
    },
    revoke: () => {
      active = false;
    },
    close: async () => {
      owner.retireAuthority();
      session.retire();
      await app.close();
    },
  };
}

test('historical open freezes actual document client, cannot save or resume editing, and shares original head after current advances', async (t) => {
  const f = await history();
  t.after(f.close);
  assert.equal(f.engine.status().frozen, true);
  assert.deepEqual(f.engine.status().head, f.head);
  assert.throws(() => f.engine.type(changed), /frozen/);
  await f.owner.document.resume();
  assert.equal(f.engine.status().frozen, true);
  await assert.rejects(f.owner.document.save(), /read-only/);
  f.advance();
  await f.session.share('pptx');
  await f.session.share('pdf');
  assert.deepEqual(f.shares, [
    { format: 'pptx', selected: f.head },
    { format: 'pdf', selected: f.head },
  ]);
  assert.equal(f.reads(), 3);
  assert.equal(f.mutations(), 0);
  await f.session.controller.flush({ retire: true });
  assert.equal(f.mutations(), 0);
});
test('historical native host rejects every write tool, another head and stale owner rather than using current', async (t) => {
  const f = await history();
  t.after(f.close);
  for (const name of [
    'app_binary_begin',
    'app_binary_append',
    'app_binary_commit',
    'app_binary_abort',
  ]) {
    await assert.rejects(f.app.callServerTool({ name, arguments: { id: 'it_deck' } }), /read-only/);
  }
  await assert.rejects(
    f.app.callServerTool({
      name: 'app_binary_export_source',
      arguments: { id: 'it_deck', version: 2, sha256: hash(changed) },
    }),
  );
  f.revoke();
  await assert.rejects(f.session.share('pdf'), /Owner retired/);
  assert.equal(f.shares.length, 0);
  assert.equal(f.mutations(), 0);
});

test('historical share failure preserves the original error without inventing an unsaved draft or blocking Close', async (t) => {
  const f = await history();
  t.after(f.close);
  f.failShare();
  await assert.rejects(f.session.share('pdf'), /Native sharing unavailable/);
  assert.equal(f.states.at(-1)?.blocked, false);
  assert.equal(f.states.at(-1)?.reason, 'export_failed');
  await f.session.controller.flush({ retire: true });
  assert.equal(f.mutations(), 0);
});

test('historical Close drains an already presented native share without flush, resume or another read', async (t) => {
  const f = await history();
  t.after(f.close);
  let release!: () => void;
  f.holdShare(
    new Promise<void>((resolve) => {
      release = resolve;
    }),
  );
  const sharing = f.session.share('pdf');
  await tick();
  assert.equal(f.shares.length, 1);
  let closed = false;
  const closing = f.session.controller.flush({ retire: true }).then(() => {
    closed = true;
  });
  await tick();
  assert.equal(closed, false);
  release();
  await sharing;
  await closing;
  assert.equal(closed, true);
  assert.equal(f.reads(), 2);
  assert.equal(f.mutations(), 0);
  assert.equal(f.engine.status().frozen, true);
});
