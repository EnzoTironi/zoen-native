import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { App } from '@modelcontextprotocol/ext-apps';
import { CanonicalBinaryHost } from './canonical-binary.ts';
import { DeckcraftDocument, attachDeckcraftDocument } from './deckcraft-document.ts';
import { dto, original, changed, enginePort } from './canonical-binary.fixtures.ts';
import { hash } from './canonical-scene.ts';

function fixture() {
  const engine = enginePort(),
    operations: string[] = [];
  let writes = 0,
    source = dto(original),
    pending: string | null = null;
  const native = new CanonicalBinaryHost({
    itemId: 'it_deck',
    read: async () => source,
    save: async () => {
      writes++;
      throw Error('Read-only test must not save');
    },
    check() {},
    id: () => 'transfer-head',
  });
  const app = {
    callServerTool: ({
      name,
      arguments: args,
    }: {
      name: string;
      arguments: Record<string, unknown>;
    }) => native.call(name, args),
  } as unknown as App;
  const options = {
    app,
    itemId: 'it_deck',
    id: () => 'head-request',
    engine: {
      ...engine,
      zoenHostRequest: async (envelope: string, bytes: Uint8Array) => {
        const op = JSON.parse(envelope).op;
        operations.push(op);
        const response = await engine.zoenHostRequest(envelope, bytes);
        if (op !== 'status' || !pending) return response;
        const metadata = JSON.parse(response.metadata);
        metadata.pendingMutationId = pending;
        return { ...response, metadata: JSON.stringify(metadata) };
      },
    },
  };
  return {
    app,
    engine,
    operations,
    options,
    writes: () => writes,
    advance: () => {
      source = dto(changed, 2);
    },
    pending: () => {
      pending = 'inflight';
    },
  };
}

test('head reports the installed Session rather than a newer Core source without focus/freeze/save', async () => {
  const f = fixture(),
    document = new DeckcraftDocument(f.options);
  await assert.rejects(document.observedHead());
  await document.open();
  f.operations.length = 0;
  f.advance();
  assert.deepEqual(await document.observedHead(), {
    itemId: 'it_deck',
    head: { version: '1', sha256: hash(original) },
  });
  assert.deepEqual(f.operations, ['status']);
  assert.equal(f.writes(), 0);
  assert.equal(f.engine.status().frozen, false);
  document.retireAuthority();
  const count = f.operations.length;
  await assert.rejects(document.observedHead());
  assert.equal(f.operations.length, count);
});
test('dirty/pending Sessions refuse observation without accepting or flushing them', async () => {
  for (const mode of ['dirty', 'pending']) {
    const f = fixture(),
      document = new DeckcraftDocument(f.options);
    await document.open();
    if (mode === 'dirty') f.engine.type(changed);
    else f.pending();
    f.operations.length = 0;
    await assert.rejects(document.observedHead());
    assert.deepEqual(f.operations, ['status']);
    assert.equal(f.writes(), 0);
    assert.equal(f.engine.accepts(), 0);
    document.retireAuthority();
  }
});
test('official document_head callback returns metadata and refuses arbitrary arguments', async () => {
  const f = fixture(),
    owner = await attachDeckcraftDocument(f.options);
  assert.ok(f.app.oncalltool);
  const reply = await f.app.oncalltool({ name: 'document_head', arguments: {} }, {} as never);
  assert.deepEqual(reply.structuredContent, {
    itemId: 'it_deck',
    head: { version: '1', sha256: hash(original) },
  });
  await assert.rejects(
    f.app.oncalltool({ name: 'document_head', arguments: { itemId: 'other' } }, {} as never),
  );
  assert.equal(f.writes(), 0);
  owner.retireAuthority();
});
