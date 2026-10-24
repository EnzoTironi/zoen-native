import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import {
  binaryEditorFormat,
  binaryExportAllowed,
  binaryExportFileType,
  WORDCRAFT_ENGINE_REVISION,
} from './binary-formats.ts';
import { CanonicalBinaryHost } from './canonical-binary.ts';
import { encode64, hash } from './canonical-scene.ts';
import { deckcraftExportFilename, streamDeckcraftExport } from './deckcraft-export.ts';
import { parseRuntimePackage } from './runtime-package.ts';
import { verifyMiniappResource } from './sandbox.ts';
import type { AppBinaryDocumentDto } from '../core/generated/AppBinaryDocumentDto';
const seed = new Uint8Array(
  readFileSync(
    new URL('../../../../crates/miniapps/resources/wordcraft-empty.docx', import.meta.url),
  ),
);
const accepted = new Uint8Array(
  readFileSync(
    new URL('../../../../crates/miniapps/src/wordcraft/fixtures/actual-wasi.docx', import.meta.url),
  ),
);
function doc(bytes = seed): AppBinaryDocumentDto {
  // Transport fixture, not evidence of a Core grant or a durable write.
  return {
    item_id: 'it_word_fixture',
    kind: 'wordcraft',
    version: 1,
    current_version: 3,
    sha256: hash(bytes),
    byte_length: bytes.length,
    data_base64: encode64(bytes),
    engine_revision: WORDCRAFT_ENGINE_REVISION,
    author_id: 'owner_fixture',
    actor_id: 'owner_fixture',
    actor_kind: 'human',
  };
}
function content(reply: Awaited<ReturnType<CanonicalBinaryHost['call']>>) {
  return reply.structuredContent as Record<string, unknown>;
}
test('Word exports only DOCX and genuine PDF; other editors and OS types stay closed', () => {
  assert.equal(binaryEditorFormat('wordcraft').maxDocumentBytes, 512 * 1024);
  assert.equal(binaryEditorFormat('wordcraft').engineRevision, WORDCRAFT_ENGINE_REVISION);
  assert.deepEqual(binaryEditorFormat('wordcraft').exports, ['docx', 'pdf']);
  for (const format of ['pptx', 'xlsx', 'html'])
    assert.ok(!binaryExportAllowed('wordcraft', format));
  assert.ok(binaryExportAllowed('wordcraft', 'docx'));
  assert.ok(binaryExportAllowed('wordcraft', 'pdf'));
  assert.ok(!binaryExportAllowed('deckcraft', 'docx'));
  assert.ok(!binaryExportAllowed('gridcraft', 'docx'));
  assert.deepEqual(binaryExportFileType('docx'), {
    mimeType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
    UTI: 'org.openxmlformats.wordprocessingml.document',
  });
  assert.equal(
    deckcraftExportFilename('', { version: '2', sha256: hash(seed) }, 'docx'),
    'document-v2.docx',
  );
  assert.equal(binaryEditorFormat().engineRevision, binaryEditorFormat('deckcraft').engineRevision);
});
test('the real bounded host keeps selected DOCX history exact and rejects a late foreign owner read', async () => {
  let live = true;
  let finish!: (doc: AppBinaryDocumentDto) => void;
  const host = new CanonicalBinaryHost({
    editor: 'wordcraft',
    itemId: 'it_word_fixture',
    id: () => 'transfer',
    check: () => {
      if (!live) throw Error('retired');
    },
    read: async (version, sha) => {
      assert.equal(version, 1);
      assert.equal(sha, hash(seed));
      return doc();
    },
    save: async () => {
      throw Error('history cannot save');
    },
  });
  const start = content(
    await host.call('app_binary_export_source', {
      id: 'it_word_fixture',
      version: 1,
      sha256: hash(seed),
    }),
  );
  assert.equal(start.version, 1);
  assert.equal(start.currentVersion, 3);
  const chunk = content(
    await host.call('app_binary_read_chunk', {
      id: 'it_word_fixture',
      transferId: start.transferId,
      index: 0,
    }),
  );
  assert.deepEqual(Buffer.from(String(chunk.data), 'base64'), Buffer.from(seed));
  const pending = new CanonicalBinaryHost({
    editor: 'wordcraft',
    itemId: 'it_word_fixture',
    id: () => 'late',
    check: () => {
      if (!live) throw Error('retired');
    },
    read: () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
    save: async () => doc(),
  });
  const task = pending.call('app_binary_read', { id: 'it_word_fixture' });
  live = false;
  finish(doc());
  await assert.rejects(task, /retired/);
  await assert.rejects(
    host.call('app_binary_read_chunk', {
      id: 'it_word_fixture',
      transferId: start.transferId,
      index: 0,
    }),
    /retired|closed/,
  );
});
test('Word refuses an oversized upload before allocating a transfer or invoking the save port', async () => {
  let allocated = 0,
    saved = 0;
  const host = new CanonicalBinaryHost({
    editor: 'wordcraft',
    itemId: 'it_word_fixture',
    check: () => {},
    id: () => {
      allocated++;
      return 'transfer';
    },
    read: async () => doc(),
    save: async () => {
      saved++;
      return doc();
    },
  });
  await assert.rejects(
    host.call('app_binary_begin', {
      id: 'it_word_fixture',
      mutationId: 'large',
      version: 1,
      sha256: hash(seed),
      bytes: 512 * 1024 + 1,
      documentSha256: hash(seed),
      intentSha256: hash(seed),
    }),
    /Invalid App proposal/,
  );
  assert.equal(allocated, 0);
  assert.equal(saved, 0);
  const wrong = new CanonicalBinaryHost({
    editor: 'wordcraft',
    itemId: 'it_word_fixture',
    check: () => {},
    id: () => 'wrong',
    read: async () => ({ ...doc(), kind: 'gridcraft' }),
    save: async () => doc(),
  });
  await assert.rejects(
    wrong.call('app_binary_read', { id: 'it_word_fixture' }),
    /Invalid canonical/,
  );
});
test('DOCX streams the real accepted writer package and releases the exact transfer after OS share', async () => {
  const head = { version: '2', sha256: hash(accepted) };
  const written: Uint8Array[] = [],
    calls: string[] = [];
  let shares = 0,
    disposed = 0;
  await streamDeckcraftExport({
    editor: 'wordcraft',
    format: 'docx',
    title: 'Report',
    head,
    check: () => {},
    call: async (name, args) => {
      calls.push(name);
      if (name === 'prepare_document_export') {
        assert.deepEqual(args.head, head);
        assert.equal(args.format, 'docx');
        return {
          structuredContent: {
            transferId: 'actual_docx',
            sourceHead: head,
            format: 'docx',
            bytes: accepted.length,
            byteLength: accepted.length,
            chunkBytes: 196608,
            sha256: hash(accepted),
          },
        };
      }
      if (name === 'read_document_export_chunk') {
        assert.equal(args.index, 0);
        return { structuredContent: { index: 0, data: encode64(accepted) } };
      }
      assert.equal(name, 'release_document_export');
      assert.equal(args.transferId, 'actual_docx');
      return {};
    },
    create: (name, format) => {
      assert.equal(name, 'Report-v2.docx');
      assert.equal(format, 'docx');
      return {
        write: async (b) => {
          written.push(b.slice());
        },
        finish: () => written.reduce((n, b) => n + b.length, 0),
        share: async () => {
          shares++;
        },
        dispose: () => {
          disposed++;
        },
      };
    },
  });
  assert.deepEqual(Buffer.concat(written), Buffer.from(accepted));
  assert.equal(shares, 1);
  assert.equal(disposed, 1);
  assert.deepEqual(calls, [
    'prepare_document_export',
    'read_document_export_chunk',
    'release_document_export',
  ]);
});
test('Word runtime keeps matching App/URI/revision and the same no-network/no-permission policy', async () => {
  const base = `https://code.example.test/wordcraft/${WORDCRAFT_ENGINE_REVISION}/`;
  const runtime = parseRuntimePackage({
    schema: 1,
    engine: 'wordcraft',
    revision: WORDCRAFT_ENGINE_REVISION,
    abi: 'zoen-host-v1',
    module: {
      url: base + 'engine.js',
      bytes: 1,
      sha256: hash(new Uint8Array([0])),
      mime: 'text/javascript',
    },
    wasm: {
      url: base + 'engine.wasm',
      bytes: 8,
      sha256: hash(new Uint8Array(8)),
      mime: 'application/wasm',
    },
  });
  const text = '<html><body><canvas id="wordcraft_canvas"></canvas></body></html>';
  const manifest = {
    id: 'wordcraft',
    name: 'Document',
    capabilities: [],
    allowedDomains: [],
    linkDomains: [],
    sha256: hash(new TextEncoder().encode(text)),
    bytes: Buffer.byteLength(text),
    runtimePackage: runtime,
  };
  const resource = {
    uri: 'ui://zoen/wordcraft/index.html',
    mime_type: 'text/html;profile=mcp-app',
    text,
    manifest_json: JSON.stringify(manifest),
    prefers_border: false,
  };
  assert.equal(
    (await verifyMiniappResource(resource, 'wordcraft')).manifest.runtimePackage?.engine,
    'wordcraft',
  );
  for (const changes of [
    { runtimePackage: { ...runtime, engine: 'deckcraft' } },
    { allowedDomains: ['example.test'] },
    { linkDomains: ['example.test'] },
    { capabilities: [{ id: 'files', purpose: 'Read local files' }] },
  ])
    await assert.rejects(
      verifyMiniappResource(
        { ...resource, manifest_json: JSON.stringify({ ...manifest, ...changes }) },
        'wordcraft',
      ),
    );
  await assert.rejects(
    verifyMiniappResource({ ...resource, uri: 'ui://zoen/gridcraft/index.html' }, 'wordcraft'),
  );
});
