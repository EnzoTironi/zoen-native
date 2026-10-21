import test from 'node:test';
import assert from 'node:assert/strict';
import { streamDeckcraftExport, deckcraftExportFilename } from './deckcraft-export.ts';
import { encode64, hash } from './canonical-scene.ts';
const CHUNK = 196_608;
const head = { version: '7', sha256: 'a'.repeat(64) };
function fixture() {
  const bytes = new Uint8Array(CHUNK * 2 + 13).fill(42);
  bytes.set(new TextEncoder().encode('%PDF-1.7'));
  let active = true;
  let disposed = 0;
  let shares = 0;
  let finished = false;
  const writes: Uint8Array[] = [];
  const calls: string[] = [];
  const names: string[] = [];
  const metadata: Record<string, unknown> = {
    transferId: 'export-one',
    bytes: bytes.length,
    byteLength: bytes.length,
    chunkBytes: CHUNK,
    sourceHead: head,
    format: 'pdf',
    sha256: hash(bytes),
  };
  const options = {
    title: '../Sky / diagram',
    format: 'pdf' as const,
    head,
    check: () => {
      if (!active) throw Error('Owner revoked');
    },
    call: async (name: string, args: Record<string, unknown>): Promise<unknown> => {
      calls.push(name);
      if (name === 'prepare_document_export') {
        assert.deepEqual(args, { format: 'pdf', head });
        return { structuredContent: metadata };
      }
      assert.equal(args.transferId, 'export-one');
      if (name === 'read_document_export_chunk') {
        const index = Number(args.index);
        return {
          structuredContent: {
            index,
            data: encode64(bytes.subarray(index * CHUNK, (index + 1) * CHUNK)),
          },
        };
      }
      assert.equal(name, 'release_document_export');
      return {};
    },
    create: (filename: string) => {
      names.push(filename);
      return {
        write: async (data: Uint8Array) => {
          writes.push(data.slice());
        },
        finish: () => {
          finished = true;
          return writes.reduce((sum, chunk) => sum + chunk.length, 0);
        },
        share: async () => {
          assert.equal(finished, true);
          shares++;
        },
        dispose: () => {
          disposed++;
        },
      };
    },
  };
  return {
    options,
    bytes,
    metadata,
    writes,
    calls,
    names,
    revoke: () => {
      active = false;
    },
    disposed: () => disposed,
    shares: () => shares,
  };
}
test('exact accepted PDF streams bounded chunks, verifies before sharing and releases every temporary owner', async () => {
  const f = fixture();
  await streamDeckcraftExport(f.options);
  assert.deepEqual(
    f.writes.map((data) => data.length),
    [CHUNK, CHUNK, 13],
  );
  assert.equal(hash(Buffer.concat(f.writes)), hash(f.bytes));
  assert.equal(f.shares(), 1);
  assert.equal(f.disposed(), 1);
  assert.equal(f.calls.at(-1), 'release_document_export');
  assert.equal(f.names[0], 'Sky  diagram-v7.pdf');
});
test('mismatched accepted head or oversized export never creates a native file', async () => {
  for (const change of [
    { sourceHead: { ...head, version: '8' } },
    { bytes: 16 * 1024 * 1024 + 1 },
    { format: 'pptx' },
    { chunkBytes: CHUNK + 1 },
  ]) {
    const f = fixture();
    Object.assign(f.metadata, change);
    await assert.rejects(streamDeckcraftExport(f.options), /Invalid accepted/);
    assert.deepEqual(f.names, []);
    assert.equal(f.calls.at(-1), 'release_document_export');
  }
});
test('corrupted or truncated chunks cannot reach the share UI and still remove the temporary file', async () => {
  for (const corrupt of [false, true]) {
    const f = fixture();
    if (corrupt) f.bytes[CHUNK] ^= 1;
    const call = f.options.call;
    f.options.call = async (name, args) => {
      const reply = await call(name, args);
      if (!corrupt && name === 'read_document_export_chunk' && args.index === 1)
        return { structuredContent: { index: 1, data: encode64(new Uint8Array(8)) } };
      return reply;
    };
    await assert.rejects(
      streamDeckcraftExport(f.options),
      corrupt ? /bytes changed/ : /chunk length/,
    );
    assert.equal(f.shares(), 0);
    assert.equal(f.disposed(), 1);
    assert.equal(f.calls.at(-1), 'release_document_export');
  }
});
test('owner revocation while a native file write is pending fences the next chunk and share', async () => {
  const f = fixture();
  const create = f.options.create;
  f.options.create = (name) => {
    const file = create(name);
    return {
      ...file,
      write: async (data) => {
        await file.write(data);
        f.revoke();
      },
    };
  };
  await assert.rejects(streamDeckcraftExport(f.options), /Owner revoked/);
  assert.equal(f.writes.length, 1);
  assert.equal(f.shares(), 0);
  assert.equal(f.disposed(), 1);
  assert.equal(f.calls.at(-1), 'release_document_export');
});
test('native share cancellation/error still closes private file and releases export transfer', async () => {
  const f = fixture();
  const create = f.options.create;
  f.options.create = (name) => ({
    ...create(name),
    share: async () => {
      throw Error('Synthetic share cancelled');
    },
  });
  await assert.rejects(streamDeckcraftExport(f.options), /share cancelled/);
  assert.equal(f.disposed(), 1);
  assert.equal(f.calls.at(-1), 'release_document_export');
});
test('filename identifies the exact selected version without path traversal', () => {
  assert.equal(deckcraftExportFilename('../../', head, 'pptx'), 'presentation-v7.pptx');
  assert.equal(deckcraftExportFilename('My slides', head, 'pptx'), 'My slides-v7.pptx');
});
