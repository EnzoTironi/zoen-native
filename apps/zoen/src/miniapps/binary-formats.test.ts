import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import {
  binaryEditorFormat,
  binaryExportAllowed,
  GRIDCRAFT_ENGINE_REVISION,
} from './binary-formats.ts';
import { CanonicalBinaryHost, type BinarySave } from './canonical-binary.ts';
import { hash } from './canonical-scene.ts';
import type { AppBinaryDocumentDto } from '../core/generated/AppBinaryDocumentDto';

const bytes = new Uint8Array(
  readFileSync(
    new URL(
      '../../../../crates/client-core/src/app_binary_documents/fixtures/managed-grid-base.xlsx',
      import.meta.url,
    ),
  ),
);
function document(): AppBinaryDocumentDto {
  return {
    item_id: 'it_grid',
    kind: 'gridcraft',
    version: 1,
    current_version: 1,
    sha256: hash(bytes),
    byte_length: bytes.length,
    data_base64: Buffer.from(bytes).toString('base64'),
    engine_revision: GRIDCRAFT_ENGINE_REVISION,
    author_id: 'person',
    actor_id: 'person',
    actor_kind: 'human',
  };
}
function content(reply: Awaited<ReturnType<CanonicalBinaryHost['call']>>) {
  return reply.structuredContent as Record<string, unknown>;
}
test('trusted binary profiles preserve Deck defaults and refuse unregistered engines or formats', () => {
  assert.equal(binaryEditorFormat().id, 'deckcraft');
  assert.equal(binaryEditorFormat('gridcraft').engineRevision, GRIDCRAFT_ENGINE_REVISION);
  for (const id of ['__proto__', 'constructor', 'unknown', null, {}])
    assert.throws(() => binaryEditorFormat(id));
  assert.ok(binaryExportAllowed('gridcraft', 'xlsx'));
  assert.ok(!binaryExportAllowed('gridcraft', 'pdf'));
  assert.ok(!binaryExportAllowed('gridcraft', 'pptx'));
  assert.ok(!binaryExportAllowed('deckcraft', 'xlsx'));
  assert.throws(() => {
    (binaryEditorFormat('gridcraft').exports as unknown as string[]).push('pdf');
  });
});
test('the existing bounded transfer host reads Grid bytes only under the matching trusted profile', async () => {
  let live = true;
  const shared = {
    itemId: 'it_grid',
    check: () => {
      if (!live) throw Error('retired');
    },
    id: () => 'transfer',
    read: async () => document(),
    save: async () => document(),
  };
  const deck = new CanonicalBinaryHost(shared);
  await assert.rejects(deck.call('app_binary_read', { id: 'it_grid' }));
  const grid = new CanonicalBinaryHost({ ...shared, editor: 'gridcraft' });
  const start = content(await grid.call('app_binary_read', { id: 'it_grid' }));
  assert.equal(start.engineRevision, GRIDCRAFT_ENGINE_REVISION);
  assert.equal(start.sha256, hash(bytes));
  const chunk = content(
    await grid.call('app_binary_read_chunk', {
      id: 'it_grid',
      transferId: start.transferId,
      index: 0,
    }),
  );
  assert.deepEqual(Buffer.from(String(chunk.data), 'base64'), Buffer.from(bytes));
  await assert.rejects(grid.call('app_binary_read', { id: 'another_item' }));
  live = false;
  await assert.rejects(
    grid.call('app_binary_read_chunk', { id: 'it_grid', transferId: start.transferId, index: 0 }),
  );
});
test('a retained Grid host cannot change format because its trusted caller options were mutated', async () => {
  const options = {
    editor: 'gridcraft' as 'gridcraft' | 'deckcraft',
    itemId: 'it_grid',
    check: () => {},
    id: () => 'transfer',
    read: async () => document(),
    save: async () => document(),
  };
  const grid = new CanonicalBinaryHost(options);
  options.editor = 'deckcraft';
  assert.equal(
    content(await grid.call('app_binary_read', { id: 'it_grid' })).engineRevision,
    GRIDCRAFT_ENGINE_REVISION,
  );
});
test('Grid shares the exact chunk/save receipt contract without accepting a foreign engine revision', async () => {
  let calls = 0;
  const base = document();
  const saves: BinarySave[] = [];
  const grid = new CanonicalBinaryHost({
    editor: 'gridcraft',
    itemId: 'it_grid',
    check: () => {},
    id: () => 'upload',
    read: async () => base,
    save: async (input) => {
      calls++;
      saves.push(input);
      return { ...base, version: 2, current_version: 2 };
    },
  });
  const begun = content(
    await grid.call('app_binary_begin', {
      id: 'it_grid',
      mutationId: 'change',
      version: 1,
      sha256: base.sha256,
      bytes: bytes.length,
      documentSha256: base.sha256,
      intentSha256: hash(new TextEncoder().encode('same input')),
    }),
  );
  await grid.call('app_binary_append', {
    id: 'it_grid',
    uploadId: begun.uploadId,
    index: 0,
    data: base.data_base64,
  });
  const accepted = await grid.call('app_binary_commit', {
    id: 'it_grid',
    uploadId: begun.uploadId,
  });
  assert.deepEqual(
    await grid.call('app_binary_commit', { id: 'it_grid', uploadId: begun.uploadId }),
    accepted,
  );
  assert.equal(calls, 1);
  assert.equal(saves[0]?.data_base64, base.data_base64);
  const wrong = new CanonicalBinaryHost({
    editor: 'gridcraft',
    itemId: 'it_grid',
    check: () => {},
    id: () => 'read',
    read: async () => ({ ...base, engine_revision: '0'.repeat(40) }),
    save: async () => base,
  });
  await assert.rejects(wrong.call('app_binary_read', { id: 'it_grid' }));
});
