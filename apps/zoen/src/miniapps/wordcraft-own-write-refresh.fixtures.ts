import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { App } from '@modelcontextprotocol/ext-apps';
import type { JSONRPCMessage, Transport } from '@modelcontextprotocol/client';
import { ClientStore } from '../core/store.ts';
import { Driver, empty, deferred, tick } from '../core/store.fixtures.ts';
import type { AppBinaryDocumentDto } from '../core/generated/AppBinaryDocumentDto';
import type { VerifiedMiniappResource } from './sandbox';
import { attachDeckcraftDocument } from './deckcraft-document.ts';
import { createDeckcraftSession } from './deckcraft-session.ts';
import { binaryEditorFormat } from './binary-formats.ts';
import { enginePort } from './canonical-binary.fixtures.ts';
import { hash } from './canonical-scene.ts';

const file = (name: string) => new Uint8Array(readFileSync(new URL(name, import.meta.url)));
export const initialWord = file('../../../../crates/miniapps/resources/wordcraft-empty.docx');
export const firstWord = file(
  '../../../../crates/miniapps/src/wordcraft/fixtures/actual-special.docx',
);
export const newerWord = file(
  '../../../../crates/miniapps/src/wordcraft/fixtures/actual-wasi.docx',
);

/** Real ClientStore + official SDK + canonical tool host. The Core driver and engine
 * port are behavioral doubles; native DOCX bytes do not make this an engine/iOS proof. */
export async function ownWriteFixture() {
  const revision = binaryEditorFormat('wordcraft').engineRevision;
  const document = (bytes: Uint8Array, version: number): AppBinaryDocumentDto => ({
    item_id: 'it_deck',
    kind: 'wordcraft',
    version,
    current_version: version,
    sha256: hash(bytes),
    byte_length: bytes.length,
    data_base64: Buffer.from(bytes).toString('base64'),
    engine_revision: revision,
    author_id: 'identity_a',
    actor_id: 'identity_a',
    actor_kind: 'human',
  });
  let source = document(initialWord, 1);
  const versions = new Map([[1, source]]);
  const receipts = new Map<string, AppBinaryDocumentDto>();
  const ack = deferred<void>(),
    notification = deferred<void>(),
    published = deferred<void>();
  let live = true,
    count = 0,
    writes = 0,
    observing = true;
  const states: { phase: string; blocked: boolean; error: string | null }[] = [];
  const events: string[] = [];
  const statusTasks: Promise<unknown>[] = [];
  const driver = new Driver();
  const store = new ClientStore(driver);
  driver.operation = async (command) => {
    if (command.op === 'snapshot') {
      events.push(`snapshot:${source.version}`);
      return empty;
    }
    if (command.op === 'app_binary_read') {
      events.push('core-read');
      const value = versions.get(command.version ?? source.version);
      assert.ok(value);
      return { ...value, current_version: source.version };
    }
    if (command.op === 'app_binary_save') {
      const prior = receipts.get(command.mutation_id);
      if (prior) return { ...prior, current_version: source.version };
      assert.equal(command.expected_version, source.version);
      assert.equal(command.expected_sha256, source.sha256);
      source = document(
        new Uint8Array(Buffer.from(command.data_base64, 'base64')),
        source.version + 1,
      );
      versions.set(source.version, source);
      receipts.set(command.mutation_id, source);
      writes++;
      events.push(`core-save:${source.version}`);
      return source;
    }
    throw Error(`Unexpected fixture command ${command.op}`);
  };
  await store.open('demo');
  let transport: Transport;
  const port = enginePort();
  const app = new App(
    { name: 'Word own-write ordering', version: '1' },
    { tools: {} },
    { autoResize: false },
  );
  const session = createDeckcraftSession({
    editor: 'wordcraft',
    itemId: 'it_deck',
    title: () => 'Word',
    verified: {
      resource: { uri: 'ui://zoen/wordcraft/index.html', text: '' },
    } as VerifiedMiniappResource,
    driver,
    mutate: (async (command) => {
      const result = await store.request(command);
      if (writes === 1) {
        published.resolve();
        await ack.promise;
      }
      events.push(`host-receipt:${source.version}`);
      return result;
    }) as typeof driver.request,
    check: () => {
      if (!live) throw Error('retired');
    },
    id: () => `own-transfer-${++count}`,
    context: () => ({}),
    send: (message) => transport.onmessage?.(message as JSONRPCMessage),
    disposeFrame: () => {},
    close: () => {},
    share: async () => {},
    onState: (value) => states.push(value),
  });
  transport = {
    start: async () => {},
    close: async () => transport.onclose?.(),
    send: async (message) => {
      void session.receive(JSON.stringify(message));
    },
  };
  await app.connect(transport);
  await tick();
  const owner = await attachDeckcraftDocument({
    editor: 'wordcraft',
    app,
    itemId: 'it_deck',
    id: () => `own-mutation-${++count}`,
    engine: {
      ...port,
      zoenHostRequest: async (envelope, bytes) => {
        const request = JSON.parse(envelope);
        events.push(request.op === 'freeze' ? `freeze:${request.input.frozen}` : request.op);
        const reply = await port.zoenHostRequest(envelope, bytes);
        const metadata = JSON.parse(reply.metadata);
        metadata.engineRevision = revision;
        return { ...reply, metadata: JSON.stringify(metadata) };
      },
    },
    onState: (phase, _error, reason) => {
      const status = app.updateModelContext({
        structuredContent: {
          zoen: { binarySave: { itemId: 'it_deck', phase, ready: true, reason } },
        },
      });
      statusTasks.push(status);
      // Retirement can close the SDK while a status response is in flight.
      // Retain every outcome for orderly fixture cleanup instead of leaking rejection.
      void status.catch(() => {});
    },
  });
  await tick();
  assert.equal(session.opened, true);
  const refreshes: Promise<void>[] = [];
  let notifiedVersion = 1;
  const stop = store.subscribe(() => {
    if (!live || !observing || source.version <= notifiedVersion) return;
    notifiedVersion = source.version;
    events.push(`published-version:${source.version}`);
    const task = session.refresh(source.version);
    void task.catch(() => {});
    refreshes.push(task);
    notification.resolve();
  });
  states.length = 0;
  events.length = 0;
  return {
    driver,
    session,
    app,
    owner,
    port,
    states,
    events,
    notification: notification.promise,
    published: published.promise,
    releaseAck: () => ack.resolve(),
    writes: () => writes,
    canonical: () => source,
    refreshes: () => Promise.all(refreshes),
    stopNotifications: () => {
      observing = false;
    },
    peer: async () => {
      source = { ...document(initialWord, source.version + 1), author_id: 'peer-author' };
      versions.set(source.version, source);
      await store.refresh();
    },
    close: async () => {
      stop();
      ack.resolve();
      live = false;
      owner.retireAuthority();
      session.retire();
      await app.close();
      await Promise.allSettled(statusTasks);
      await store.close({ force: true });
    },
  };
}
