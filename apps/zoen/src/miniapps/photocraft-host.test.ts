import test from 'node:test';
import assert from 'node:assert/strict';
import { binaryExportAllowed, binaryExportFileType } from './binary-formats.ts';
import { streamDeckcraftExport } from './deckcraft-export.ts';
import { encode64, hash } from './canonical-scene.ts';

test('image native and flattened export have closed formats and OS identities', () => {
  assert.equal(binaryExportAllowed('photocraft', 'png'), true);
  assert.equal(binaryExportAllowed('photocraft', 'pcraft'), true);
  assert.equal(binaryExportAllowed('photocraft', 'pdf'), false);
  assert.equal(binaryExportAllowed('wordcraft', 'png'), false);
  assert.equal(binaryExportFileType('png').mimeType, 'image/png');
});

test('PNG signatures reach sharing only as explicit PNG; wrong formats release the transfer', async () => {
  const png = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 0]);
  const zip = new Uint8Array([80, 75, 3, 4, 0, 0, 0, 0]);
  for (const [format, bytes, valid] of [
    ['png', png, true],
    ['png', zip, false],
    ['pcraft', png, false],
  ] as const) {
    const calls: string[] = [];
    let shared = 0;
    let disposed = 0;
    let written = 0;
    const head = { version: '2', sha256: 'a'.repeat(64) };
    const run = streamDeckcraftExport({
      editor: 'photocraft',
      title: 'Image',
      format,
      head,
      check: () => {},
      call: async (name) => {
        calls.push(name);
        if (name === 'prepare_document_export')
          return {
            structuredContent: {
              transferId: 'photo-export',
              sourceHead: head,
              format,
              bytes: bytes.length,
              byteLength: bytes.length,
              sha256: hash(bytes),
              chunkBytes: 196608,
            },
          };
        if (name === 'read_document_export_chunk')
          return { structuredContent: { index: 0, data: encode64(bytes) } };
        assert.equal(name, 'release_document_export');
        return {};
      },
      create: () => ({
        write: async (chunk) => {
          written += chunk.length;
        },
        finish: () => written,
        share: async () => {
          shared++;
        },
        dispose: () => {
          disposed++;
        },
      }),
    });
    if (valid) await run;
    else await assert.rejects(run, /export format/);
    assert.equal(shared, valid ? 1 : 0);
    assert.equal(written, valid ? bytes.length : 0);
    assert.equal(disposed, 1);
    assert.equal(calls.at(-1), 'release_document_export');
  }
});
