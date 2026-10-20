import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { App } from '@modelcontextprotocol/ext-apps';
import type { JSONRPCMessage, Transport } from '@modelcontextprotocol/client';
import type { AppBinaryDocumentDto } from '../core/generated/AppBinaryDocumentDto';
import type { Command, CoreDriver } from '../core/types';
import { createNativeCanonicalBinaryHost } from './canonical-binary-native.ts';
import { MiniappSession } from './protocol.ts';
import {
  canonicalBinaryTools,
  DECKCRAFT_ENGINE_REVISION,
  type BinarySave,
} from './canonical-binary.ts';
import { DeckcraftDocument, type DeckcraftBindings } from './deckcraft-document.ts';
import { hash, encode64, CANONICAL_SCENE_CHUNK_BYTES as CHUNK } from './canonical-scene.ts';
const original = new Uint8Array(
  readFileSync(
    new URL(
      '../../../../crates/client-core/src/app_binary_documents/fixtures/original.deckcraft',
      import.meta.url,
    ),
  ),
);
const changed = new Uint8Array(
  readFileSync(
    new URL(
      '../../../../crates/client-core/src/app_binary_documents/fixtures/changed.deckcraft',
      import.meta.url,
    ),
  ),
);
const scope = { artifactId: 'it_deck', actorId: 'identity_a', actorKind: 'human' };
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
const intent = hash(new TextEncoder().encode('human checkpoint'));
function dto(bytes: Uint8Array, version = 1): AppBinaryDocumentDto {
  return {
    item_id: 'it_deck',
    kind: 'deckcraft',
    version,
    current_version: version,
    sha256: hash(bytes),
    byte_length: bytes.length,
    data_base64: Buffer.from(bytes).toString('base64'),
    engine_revision: DECKCRAFT_ENGINE_REVISION,
    author_id: scope.actorId,
    actor_id: scope.actorId,
    actor_kind: 'human',
  };
}
function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
async function fixture(
  overrides: {
    save?: (input: BinarySave) => Promise<AppBinaryDocumentDto>;
    read?: (v?: number, sha?: string) => Promise<AppBinaryDocumentDto>;
  } = {},
) {
  let active = true;
  let ids = 0;
  let doc = dto(original);
  const versions = new Map([[1, doc]]);
  const receipts = new Map<string, AppBinaryDocumentDto>();
  const saves: BinarySave[] = [];
  const save = async (input: BinarySave) => {
    saves.push(input);
    if (overrides.save) return overrides.save(input);
    const prior = receipts.get(input.mutation_id);
    if (prior) return { ...prior, current_version: doc.version };
    if (input.expected_version !== doc.version || input.expected_sha256 !== doc.sha256)
      throw new Error('stale native head');
    const bytes = new Uint8Array(Buffer.from(input.data_base64, 'base64'));
    doc = dto(bytes, doc.version + 1);
    versions.set(doc.version, doc);
    receipts.set(input.mutation_id, doc);
    return doc;
  };
  const read =
    overrides.read ??
    (async (v?: number, sha?: string) => {
      const result = versions.get(v ?? doc.version)!;
      if (!result || (sha && sha !== result.sha256)) throw new Error('stale native source');
      return { ...result, current_version: doc.version };
    });
  const binary = createNativeCanonicalBinaryHost({
    itemId: 'it_deck',
    check: () => {
      if (!active) throw new Error('owner retired');
    },
    id: () => `transfer_${++ids}`,
    driver: {
      request: (async (command: Command) => {
        assert.equal(command.op, 'app_binary_read');
        if (command.op !== 'app_binary_read') throw new Error('unexpected native read operation');
        assert.equal(command.item_id, 'it_deck');
        return read(command.version ?? undefined, command.expected_sha256 ?? undefined);
      }) as CoreDriver['request'],
    },
    mutate: (async (command: Command) => {
      assert.equal(command.op, 'app_binary_save');
      if (command.op !== 'app_binary_save') throw new Error('unexpected native mutation');
      const { op: _op, ...input } = command;
      return save(input);
    }) as CoreDriver['request'],
  });
  let transport: Transport | undefined;
  const session = new MiniappSession({
    send: (message) => transport?.onmessage?.(message as JSONRPCMessage),
    resourceUri: 'ui://zoen/deckcraft/index.html',
    tools: canonicalBinaryTools,
    initialize: () => ({
      hostInfo: { name: 'zoen', version: 'test' },
      hostCapabilities: { serverTools: {}, serverResources: {} },
      hostContext: { theme: 'dark', displayMode: 'fullscreen' },
    }),
    context: () => ({ theme: 'dark', displayMode: 'fullscreen' }),
    state: () => ({ content: [], structuredContent: { checkpointId: 'it_deck' } }),
    resource: async () => ({ contents: [] }),
    call: (name, args) => binary.call(name, args),
    updateModelContext: async () => ({}),
    openLink: async () => ({}),
    share: async () => ({}),
    native: async () => ({}),
    haptic: () => {},
    resize: () => {},
    close: () => {},
  });
  const app = new App(
    { name: 'DeckCraft contract', version: '0.9.0' },
    { tools: {} },
    { autoResize: false, strict: true },
  );
  transport = {
    start: async () => {},
    close: async () => {},
    send: async (message) => {
      void session.receive(JSON.stringify(message));
    },
  };
  await app.connect(transport);
  await tick();
  const tool = async (name: string, args: Record<string, unknown> = {}) => {
    const result = await app.callServerTool({ name, arguments: { id: 'it_deck', ...args } });
    if (result.isError) throw new Error('tool refused');
    assert.ok(result.structuredContent && typeof result.structuredContent === 'object');
    return result.structuredContent as Record<string, unknown>;
  };
  return {
    app,
    binary,
    session,
    saves,
    tool,
    current: () => doc,
    retire: () => {
      active = false;
      binary.dispose();
    },
    dispose: () => {
      binary.dispose();
      session.dispose();
      void app.close();
    },
  };
}
/** Behavioral engine port for JS orchestration tests only. Real Session/PPTX/PDF
 * execution is separately covered by frozen v154 engine tests, not this port. */
function enginePort(): DeckcraftBindings & {
  type: (bytes: Uint8Array) => void;
  status: () => Record<string, unknown>;
  accepts: () => number;
} {
  let current: Uint8Array = original.slice();
  let accepted: Uint8Array = original.slice();
  let binding = scope;
  let version = '1';
  let frozen = false;
  let retired = false;
  let pending: { id: string; bytes: Uint8Array } | null = null;
  let callback: ((status: string) => void) | undefined;
  let accepts = 0;
  const status = () => ({
    engineRevision: DECKCRAFT_ENGINE_REVISION,
    scope: binding,
    head: { version, sha256: hash(accepted) },
    dirty: hash(current) !== hash(accepted),
    frozen,
    pendingMutationId: pending?.id ?? null,
  });
  const reply = (metadata: Record<string, unknown>, bytes: Uint8Array = new Uint8Array()) => ({
    metadata: JSON.stringify(metadata),
    bytes,
  });
  return {
    type: (bytes) => {
      if (frozen || retired) throw new Error('input frozen');
      current = bytes.slice();
      callback?.(JSON.stringify(status()));
    },
    status,
    accepts: () => accepts,
    zoenHostOnChange: (fn) => {
      callback = fn;
    },
    zoenHostRetire: () => {
      retired = true;
      frozen = true;
      callback = undefined;
    },
    zoenHostRequest: async (envelope, bytes) => {
      if (retired) throw new Error('engine retired');
      const { op, input } = JSON.parse(envelope);
      if (op === 'open') {
        current = bytes.slice();
        accepted = bytes.slice();
        binding = input.scope;
        version = input.head.version;
        return reply(status());
      }
      if (op === 'status') return reply(status());
      if (op === 'freeze') {
        frozen = input.frozen;
        return reply(status());
      }
      if (op === 'snapshot') {
        pending ??= { id: input.mutationId, bytes: current.slice() };
        return reply(
          {
            engineRevision: DECKCRAFT_ENGINE_REVISION,
            scope: binding,
            mutationId: pending.id,
            base: input.base,
            sha256: hash(pending.bytes),
            byteLength: pending.bytes.length,
            operation: { kind: 'humanCheckpoint' },
          },
          pending.bytes.slice(),
        );
      }
      if (op === 'accept') {
        if (pending) {
          accepted = pending.bytes;
          pending = null;
          version = input.head.version;
          accepts++;
        }
        return reply(status());
      }
      throw new Error(`unsupported port operation ${op}`);
    },
  };
}

test('real official SDK reaches the production scoped binary host with native-shape bytes and exact receipt', async (t) => {
  const f = await fixture();
  t.after(f.dispose);
  await assert.rejects(f.tool('app_binary_commit'), /Invalid App proposal identifier/);
  const read = await f.tool('app_binary_read');
  assert.equal(read.sha256, hash(original));
  assert.deepEqual(read.scope, scope);
  const chunk = await f.tool('app_binary_read_chunk', { transferId: read.transferId, index: 0 });
  assert.deepEqual(Buffer.from(chunk.data as string, 'base64'), Buffer.from(original));
  const begin = await f.tool('app_binary_begin', {
    mutationId: 'save_once',
    version: 1,
    sha256: hash(original),
    bytes: changed.length,
    documentSha256: hash(changed),
    intentSha256: intent,
  });
  await f.tool('app_binary_append', {
    uploadId: begin.uploadId,
    index: 0,
    data: encode64(changed),
  });
  const result = await f.tool('app_binary_commit', { uploadId: begin.uploadId });
  assert.deepEqual(result.head, { version: '2', sha256: hash(changed) });
  assert.equal(f.saves[0].intent_sha256, intent);
  await f.tool('app_binary_commit', { uploadId: begin.uploadId });
  assert.equal(f.saves.length, 1);
});
test('same production document client freezes before final snapshot and requires explicit resume', async (t) => {
  const f = await fixture();
  t.after(f.dispose);
  const engine = enginePort();
  let ids = 0;
  const document = new DeckcraftDocument({
    app: f.app,
    engine,
    itemId: 'it_deck',
    id: () => `mutation_${++ids}`,
  });
  t.after(() => document.retireAuthority());
  await document.open();
  engine.type(changed);
  await document.flush();
  assert.equal(f.current().version, 2);
  assert.equal(engine.status().dirty, false);
  assert.equal(engine.status().frozen, true);
  assert.throws(() => engine.type(original), /frozen/);
  await document.resume();
  engine.type(original);
  await document.flush();
  assert.equal(f.current().version, 3);
});
test('lost durable response preserves exact pending proposal and newer manual typing until retry is accepted', async (t) => {
  const stored = new Map<string, AppBinaryDocumentDto>();
  let calls = 0;
  let head = 1;
  const f = await fixture({
    save: async (input) => {
      calls++;
      const prior = stored.get(input.mutation_id);
      if (prior) return { ...prior, current_version: head };
      const saved = dto(new Uint8Array(Buffer.from(input.data_base64, 'base64')), ++head);
      stored.set(input.mutation_id, saved);
      if (calls === 1) throw new Error('synthetic response lost after durable commit');
      return saved;
    },
  });
  t.after(f.dispose);
  const engine = enginePort();
  let ids = 0;
  const document = new DeckcraftDocument({
    app: f.app,
    engine,
    itemId: 'it_deck',
    id: () => `mutation_${++ids}`,
  });
  t.after(() => document.retireAuthority());
  await document.open();
  engine.type(changed);
  await assert.rejects(document.save());
  engine.type(original);
  await document.flush();
  assert.equal(head, 3);
  assert.equal(calls, 3);
  assert.equal(engine.accepts(), 2);
  assert.deepEqual(f.saves[0], f.saves[1]);
  assert.equal(f.saves[2].expected_version, 2);
  assert.equal(engine.status().dirty, false);
  assert.equal(engine.status().frozen, true);
});
test('owner retirement while saving cannot accept into a retired engine or publish into a replacement binding', async (t) => {
  const started = deferred<void>();
  const release = deferred<AppBinaryDocumentDto>();
  const f = await fixture({
    save: async () => {
      started.resolve();
      return release.promise;
    },
  });
  t.after(f.dispose);
  const engine = enginePort();
  const document = new DeckcraftDocument({
    app: f.app,
    engine,
    itemId: 'it_deck',
    id: () => 'mutation',
  });
  await document.open();
  engine.type(changed);
  const saving = document.save();
  const rejected = assert.rejects(saving);
  await started.promise;
  f.retire();
  document.retireAuthority();
  release.resolve(dto(changed, 2));
  await rejected;
  assert.equal(engine.accepts(), 0);
  await assert.rejects(document.save(), /closed/);
});
test('wrong artifact, spoofed author, reordered/mutated chunks and mismatched base cannot escape this binding', async (t) => {
  const f = await fixture();
  t.after(f.dispose);
  await assert.rejects(f.tool('app_binary_read', { id: 'other' }));
  await assert.rejects(f.tool('app_binary_read', { actorId: 'spoof' }));
  const args = {
    mutationId: 'm',
    version: 1,
    sha256: hash(original),
    bytes: changed.length,
    documentSha256: hash(changed),
    intentSha256: intent,
  };
  const begin = await f.tool('app_binary_begin', args);
  await assert.rejects(f.tool('app_binary_begin', { ...args, version: 2 }));
  await assert.rejects(
    f.tool('app_binary_append', { uploadId: begin.uploadId, index: 1, data: encode64(changed) }),
  );
  await f.tool('app_binary_append', {
    uploadId: begin.uploadId,
    index: 0,
    data: encode64(changed),
  });
  const modified = changed.slice();
  modified[0] ^= 1;
  await assert.rejects(
    f.tool('app_binary_append', { uploadId: begin.uploadId, index: 0, data: encode64(modified) }),
  );
  assert.equal(f.saves.length, 0);
});
test('a late read cannot replace a newer exact export source and selected hash is mandatory', async (t) => {
  const old = deferred<AppBinaryDocumentDto>();
  let reads = 0;
  const f = await fixture({ read: async () => (++reads === 1 ? old.promise : dto(changed, 2)) });
  t.after(f.dispose);
  const first = f.binary.call('app_binary_read', { id: 'it_deck' });
  const refused = assert.rejects(first, /replaced/);
  const source = await f.binary.call('app_binary_export_source', {
    id: 'it_deck',
    version: 2,
    sha256: hash(changed),
  });
  old.resolve(dto(original));
  await refused;
  const transferId = (source.structuredContent as Record<string, unknown>).transferId;
  const chunk = await f.binary.call('app_binary_read_chunk', {
    id: 'it_deck',
    transferId,
    index: 0,
  });
  assert.equal((chunk.structuredContent as Record<string, unknown>).data, encode64(changed));
  await assert.rejects(f.tool('app_binary_export_source', { version: 2 }));
});
test('chunk tools remain below the unchanged guest message ceiling', async (t) => {
  const f = await fixture();
  t.after(f.dispose);
  const bytes = new Uint8Array(CHUNK);
  bytes.fill(77);
  const encoded = JSON.stringify({
    jsonrpc: '2.0',
    id: 1,
    method: 'tools/call',
    params: {
      name: 'app_binary_append',
      arguments: { id: 'it_deck', uploadId: 'upload', index: 0, data: encode64(bytes) },
    },
  });
  assert(encoded.length < 1_500_000);
  assert(encoded.length < 263_000);
});

test('abort distinguishes locally cancelled, committed and unknown writes without discarding a lost-response intent', async (t) => {
  let durable: AppBinaryDocumentDto | null = null;
  let attempts = 0;
  const f = await fixture({
    save: async (input) => {
      attempts++;
      if (durable) return durable;
      durable = dto(new Uint8Array(Buffer.from(input.data_base64, 'base64')), 2);
      throw new Error('synthetic lost reply after committed bytes');
    },
  });
  t.after(f.dispose);
  const args = {
    mutationId: 'unknown',
    version: 1,
    sha256: hash(original),
    bytes: changed.length,
    documentSha256: hash(changed),
    intentSha256: intent,
  };
  const begin = await f.tool('app_binary_begin', args);
  await f.tool('app_binary_append', {
    uploadId: begin.uploadId,
    index: 0,
    data: encode64(changed),
  });
  await assert.rejects(f.tool('app_binary_commit', { uploadId: begin.uploadId }));
  assert.deepEqual(await f.tool('app_binary_abort', { uploadId: begin.uploadId }), {
    aborted: false,
    outcome: 'unknown',
    uploadId: begin.uploadId,
    mutationId: 'unknown',
  });
  await assert.rejects(f.tool('app_binary_begin', { ...args, mutationId: 'replacement' }));
  const retried = await f.tool('app_binary_commit', { uploadId: begin.uploadId });
  assert.equal((retried.head as { version: string }).version, '2');
  assert.equal(attempts, 2);
  assert.deepEqual(f.saves[0], f.saves[1]);
  assert.equal(
    (await f.tool('app_binary_abort', { uploadId: begin.uploadId })).outcome,
    'committed',
  );
  const fresh = await fixture();
  t.after(fresh.dispose);
  const local = await fresh.tool('app_binary_begin', args);
  assert.equal(
    (await fresh.tool('app_binary_abort', { uploadId: local.uploadId })).outcome,
    'locallyCancelled',
  );
  assert.equal(fresh.saves.length, 0);
});

test('a rejected save before any durable receipt is conservatively unknown and retains exact retry input', async (t) => {
  let refuse = true;
  const f = await fixture({
    save: async (input) => {
      if (refuse) throw new Error('synthetic pre-write denial');
      return dto(new Uint8Array(Buffer.from(input.data_base64, 'base64')), 2);
    },
  });
  t.after(f.dispose);
  const begin = await f.tool('app_binary_begin', {
    mutationId: 'denied',
    version: 1,
    sha256: hash(original),
    bytes: changed.length,
    documentSha256: hash(changed),
    intentSha256: intent,
  });
  await f.tool('app_binary_append', {
    uploadId: begin.uploadId,
    index: 0,
    data: encode64(changed),
  });
  await assert.rejects(f.tool('app_binary_commit', { uploadId: begin.uploadId }));
  assert.equal((await f.tool('app_binary_abort', { uploadId: begin.uploadId })).outcome, 'unknown');
  refuse = false;
  await f.tool('app_binary_commit', { uploadId: begin.uploadId });
  assert.deepEqual(f.saves[0], f.saves[1]);
});
