import type { JSONRPCMessage, Transport } from '@modelcontextprotocol/client';
import { App } from '@modelcontextprotocol/ext-apps';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import type { AppBinaryDocumentDto } from '../core/generated/AppBinaryDocumentDto';
import type { Command, CoreDriver } from '../core/types';
import { createNativeCanonicalBinaryHost } from './canonical-binary-native.ts';
import {
  canonicalBinaryTools,
  DECKCRAFT_ENGINE_REVISION,
  type BinarySave,
} from './canonical-binary.ts';
import { hash } from './canonical-scene.ts';
import { type DeckcraftBindings } from './deckcraft-document.ts';
import { MiniappSession } from './protocol.ts';

export const original = new Uint8Array(
  readFileSync(
    new URL(
      '../../../../crates/client-core/src/app_binary_documents/fixtures/original.deckcraft',
      import.meta.url,
    ),
  ),
);
export const changed = new Uint8Array(
  readFileSync(
    new URL(
      '../../../../crates/client-core/src/app_binary_documents/fixtures/changed.deckcraft',
      import.meta.url,
    ),
  ),
);
export const scope = { artifactId: 'it_deck', actorId: 'identity_a', actorKind: 'human' };
export const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
export const intent = hash(new TextEncoder().encode('human checkpoint'));
export function dto(bytes: Uint8Array, version = 1): AppBinaryDocumentDto {
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
export function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
export async function fixture(
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
export function enginePort(): DeckcraftBindings & {
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
