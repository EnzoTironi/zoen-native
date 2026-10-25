import test from 'node:test';
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { App } from '@modelcontextprotocol/ext-apps';
import type { JSONRPCMessage, Transport } from '@modelcontextprotocol/client';
import type { CoreDriver } from '../core/types';
import type { VerifiedMiniappResource } from './sandbox';
import { attachDeckcraftDocument } from './deckcraft-document.ts';
import { createDeckcraftSession } from './deckcraft-session.ts';
import { binaryEditorFormat } from './binary-formats.ts';
import { dto, enginePort, tick } from './canonical-binary.fixtures.ts';
import { hash } from './canonical-scene.ts';

// SDK orchestration only. Actual Session/Undo/pixels are tested in each engine adapter.
async function refreshFixture(
  editor: 'wordcraft' | 'photocraft',
  original: Uint8Array,
  changed: Uint8Array,
) {
  const format = binaryEditorFormat(editor);
  const revision = format.engineRevision;
  let source = { ...dto(original), kind: editor, engine_revision: revision };
  let reads = 0,
    mutations = 0;
  let live = true;
  let transport: Transport;
  let heldRead: Promise<void> = Promise.resolve();
  let beforeFreeze: (() => Promise<void>) | undefined;
  const states: { blocked: boolean; error: string | null }[] = [];
  const events: string[] = [];
  const port = enginePort();
  const app = new App(
    { name: `${editor} refresh`, version: '1' },
    { tools: {} },
    { autoResize: false },
  );
  const session = createDeckcraftSession({
    editor,
    itemId: 'it_deck',
    title: () => editor,
    verified: {
      resource: {
        uri: format.resourceUri,
        text: '',
        mime_type: 'text/html;profile=mcp-app',
      },
    } as VerifiedMiniappResource,
    driver: {
      request: (async (command) => {
        assert.equal(command.op, 'app_binary_read');
        events.push('read');
        reads++;
        await heldRead;
        return source;
      }) as CoreDriver['request'],
    },
    mutate: (async () => {
      mutations++;
      throw Error('No writes during refresh');
    }) as CoreDriver['request'],
    check: () => {
      if (!live) throw Error('retired');
    },
    id: () => 'refresh-transfer',
    context: () => ({}),
    send: (message) => transport.onmessage?.(message as JSONRPCMessage),
    disposeFrame: () => {},
    close: () => {},
    share: async () => {},
    onState: (state) => states.push(state),
  });
  transport = {
    start: async () => {},
    close: async () => {
      transport.onclose?.();
    },
    send: async (message) => {
      void session.receive(JSON.stringify(message));
    },
  };
  await app.connect(transport);
  await tick();
  const owner = await attachDeckcraftDocument({
    editor,
    app,
    itemId: 'it_deck',
    id: () => 'refresh-mutation',
    engine: {
      ...port,
      zoenHostRequest: async (envelope, bytes) => {
        const request = JSON.parse(envelope);
        events.push(request.op === 'freeze' ? `freeze:${request.input.frozen}` : request.op);
        if (request.op === 'freeze' && request.input.frozen) await beforeFreeze?.();
        const response = await port.zoenHostRequest(envelope, bytes);
        const metadata = JSON.parse(response.metadata);
        metadata.engineRevision = revision;
        return { ...response, metadata: JSON.stringify(metadata) };
      },
    },
  });
  await app.updateModelContext({
    structuredContent: { zoen: { binarySave: { itemId: 'it_deck', phase: 'saved', ready: true } } },
  });
  events.length = 0;
  return {
    session,
    owner,
    port,
    events,
    states,
    advance: () => {
      source = { ...dto(changed, 2), kind: editor, engine_revision: revision };
    },
    foreign: () => {
      source = { ...source, actor_id: 'another-human' };
    },
    holdRead: (promise: Promise<void>) => {
      heldRead = promise;
    },
    freezeInput: (callback: () => Promise<void>) => {
      beforeFreeze = callback;
    },
    reads: () => reads,
    mutations: () => mutations,
    close: async () => {
      owner.retireAuthority();
      live = false;
      session.retire();
      await app.close();
    },
  };
}

for (const editor of ['wordcraft', 'photocraft'] as const) {
  const read = (path: string) => new Uint8Array(readFileSync(new URL(path, import.meta.url)));
  const original = read(
    editor === 'wordcraft'
      ? '../../../../crates/miniapps/resources/wordcraft-empty.docx'
      : '../../../../crates/miniapps/src/photocraft/fixtures/native-asymmetric.pcraft',
  );
  const changed = read(
    editor === 'wordcraft'
      ? '../../../../crates/miniapps/src/wordcraft/fixtures/actual-special.docx'
      : '../../../../crates/miniapps/src/photocraft/fixtures/native-rotated.pcraft',
  );
  const fixture = () => refreshFixture(editor, original, changed);
  test(`${editor}: official SDK refresh drains input and opens the new canonical head on the same binding without writes`, async (t) => {
    const f = await fixture();
    t.after(f.close);
    f.advance();
    await f.session.refresh(2);
    assert.deepEqual(f.events, ['status', 'freeze:true', 'status', 'read', 'open', 'freeze:false']);
    assert.deepEqual(f.port.status().head, { version: '2', sha256: hash(changed) });
    assert.equal(f.port.status().frozen, false);
    assert.equal(f.states.at(-1)?.blocked, false);
    assert.equal(f.mutations(), 0);
    assert.ok(f.session.controller.observedHead);
    assert.deepEqual(await f.session.controller.observedHead(), {
      itemId: 'it_deck',
      version: 2,
      sha256: hash(changed),
    });
    await f.session.refresh();
    assert.equal(
      f.events.filter((event) => event === 'open').length,
      1,
      'own-save notification must not create another Undo step',
    );
  });

  test(`${editor}: input admitted before pause is checked before refresh and its draft is preserved without another read`, async (t) => {
    const f = await fixture();
    t.after(f.close);
    f.advance();
    f.freezeInput(async () => {
      f.port.type(changed);
    });
    await assert.rejects(f.session.refresh(), /local draft/);
    assert.deepEqual(f.events, ['freeze:true', 'status', 'freeze:false']);
    assert.equal(f.port.status().dirty, true);
    assert.equal(f.port.status().frozen, false);
    assert.deepEqual(f.port.status().head, { version: '1', sha256: hash(original) });
    assert.equal(f.reads(), 1);
    assert.equal(f.mutations(), 0);
    assert.equal(f.states.at(-1)?.blocked, true);
  });

  test(`${editor}: an engine pending receipt cannot be overwritten even without dirty text`, async (t) => {
    const f = await fixture();
    t.after(f.close);
    f.advance();
    f.freezeInput(async () => {
      await f.port.zoenHostRequest(
        JSON.stringify({
          op: 'snapshot',
          input: { mutationId: 'pending', base: f.port.status().head },
        }),
        new Uint8Array(),
      );
    });
    await assert.rejects(f.session.refresh(), /local draft/);
    assert.equal(f.port.status().pendingMutationId, 'pending');
    assert.equal(f.reads(), 1);
    assert.equal(f.mutations(), 0);
  });

  test(`${editor}: authority mismatch and retirement during a download never reopen or publish late bytes`, async (t) => {
    const foreign = await fixture();
    t.after(foreign.close);
    foreign.advance();
    foreign.foreign();
    await assert.rejects(foreign.session.refresh(), /authority changed/);
    assert.equal(foreign.events.includes('open'), false);
    assert.equal(foreign.mutations(), 0);
    const retiring = await fixture();
    t.after(retiring.close);
    retiring.advance();
    let release!: () => void;
    retiring.holdRead(
      new Promise<void>((resolve) => {
        release = resolve;
      }),
    );
    const refreshing = retiring.session.refresh();
    await tick();
    assert.equal(retiring.port.status().frozen, true);
    assert.throws(
      () => retiring.port.type(changed),
      /input frozen/,
      'a real higher-head download still fences engine input without hiding the frame',
    );
    await retiring.close();
    release();
    await assert.rejects(refreshing);
    assert.equal(retiring.events.includes('open'), false);
    assert.equal(retiring.mutations(), 0);
  });

  test(`${editor}: own accepted-head notification leaves newer human input editable instead of treating it as a peer conflict`, async (t) => {
    const f = await fixture();
    t.after(f.close);
    f.port.type(changed);
    await f.session.refresh(1);
    assert.deepEqual(f.events, ['status']);
    assert.equal(f.port.status().dirty, true);
    assert.equal(f.port.status().frozen, false);
    assert.equal(f.states.at(-1)?.blocked, false);
    assert.equal(f.reads(), 1);
    assert.equal(f.mutations(), 0);
  });
}
