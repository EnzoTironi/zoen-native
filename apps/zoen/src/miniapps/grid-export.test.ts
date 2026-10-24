import test from 'node:test';
import assert from 'node:assert/strict';
import {
  streamDeckcraftExport,
  deckcraftExportFilename,
  type DeckcraftExportOptions,
} from './deckcraft-export.ts';
import { encode64, hash } from './canonical-scene.ts';
function fixture(editor: 'deckcraft' | 'gridcraft', format: 'pptx' | 'pdf' | 'xlsx') {
  const bytes = new Uint8Array([0x50, 0x4b, 3, 4, 1, 2, 3, 4, 5]);
  const head = { version: '3', sha256: 'a'.repeat(64) };
  let created = 0,
    shared = 0,
    disposed = 0;
  const written: Uint8Array[] = [];
  const calls: string[] = [];
  const options: DeckcraftExportOptions = {
    editor,
    format,
    title: '../Budget / draft',
    head,
    check: () => {},
    call: async (name, args) => {
      calls.push(name);
      if (name === 'prepare_document_export')
        return {
          structuredContent: {
            transferId: 'grid-export',
            bytes: bytes.length,
            byteLength: bytes.length,
            chunkBytes: 196608,
            sourceHead: head,
            format,
            sha256: hash(bytes),
          },
        };
      if (name === 'read_document_export_chunk') {
        assert.equal(args.index, 0);
        return { structuredContent: { index: 0, data: encode64(bytes) } };
      }
      assert.equal(name, 'release_document_export');
      return {};
    },
    create: (filename, kind) => {
      created++;
      assert.equal(filename, 'Budget  draft-v3.xlsx');
      assert.equal(kind, 'xlsx');
      return {
        write: async (b) => {
          written.push(b.slice());
        },
        finish: () => written.reduce((n, b) => n + b.length, 0),
        share: async () => {
          shared++;
        },
        dispose: () => {
          disposed++;
        },
      };
    },
  };
  return { options, bytes, written, calls, counts: () => ({ created, shared, disposed }) };
}
test('explicit Grid XLSX reuses bounded accepted-version stream and verified native-file lifetime', async () => {
  const f = fixture('gridcraft', 'xlsx');
  await streamDeckcraftExport(f.options);
  assert.deepEqual(Buffer.concat(f.written), Buffer.from(f.bytes));
  assert.deepEqual(f.counts(), { created: 1, shared: 1, disposed: 1 });
  assert.deepEqual(f.calls, [
    'prepare_document_export',
    'read_document_export_chunk',
    'release_document_export',
  ]);
  assert.equal(deckcraftExportFilename('', f.options.head, 'xlsx'), 'workbook-v3.xlsx');
});
test('Grid refuses presentation/PDF exports and legacy Deck refuses XLSX before any RPC or file', async () => {
  for (const [editor, format] of [
    ['gridcraft', 'pdf'],
    ['gridcraft', 'pptx'],
    ['deckcraft', 'xlsx'],
  ] as const) {
    const f = fixture(editor, format);
    await assert.rejects(streamDeckcraftExport(f.options), /Invalid accepted/);
    assert.deepEqual(f.calls, []);
    assert.deepEqual(f.counts(), { created: 0, shared: 0, disposed: 0 });
  }
  const f = fixture('gridcraft', 'xlsx');
  delete f.options.editor;
  await assert.rejects(streamDeckcraftExport(f.options), /Invalid accepted/);
  assert.deepEqual(f.calls, []);
});
