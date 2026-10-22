// Opt-in, independently keyed browser fixture. No product route or QA account.
import { BrowserVault } from '../vault.ts';
import { CoreWorker } from '../worker-client.ts';
import { Messages, type MessagesState } from '../messages.ts';
import type { StoredSession } from '../session.ts';
declare const FIXTURE_WORKER_INTEGRITY: string;
declare global {
  interface Window {
    cursorResult?: unknown;
    cursorEntry?: boolean;
  }
}
window.cursorEntry = true;
let phase = 'setup';
class ContractFailure extends Error {}
const check = (condition: unknown, label: string): void => {
  if (!condition) throw new ContractFailure(label);
};
const pause = () => new Promise<void>((resolve) => setTimeout(resolve, 25));
async function until(condition: () => boolean, milliseconds: number): Promise<void> {
  const deadline = performance.now() + milliseconds;
  while (!condition()) {
    if (performance.now() >= deadline)
      throw new ContractFailure('Synthetic browser frontier timed out');
    await pause();
  }
}
async function seedSyntheticVault(device: string): Promise<void> {
  // Fixed test-only seed113 matches the Rust HostSigner; never imported from a profile.
  const pkcs8 = new Uint8Array([
    48,
    46,
    2,
    1,
    0,
    48,
    5,
    6,
    3,
    43,
    101,
    112,
    4,
    34,
    4,
    32,
    ...new Uint8Array(32).fill(113),
  ]);
  const privateKey = await crypto.subtle.importKey('pkcs8', pkcs8, 'Ed25519', false, ['sign']);
  pkcs8.fill(0);
  const publicKey = await crypto.subtle.importKey(
    'raw',
    Uint8Array.from(device.match(/../g) ?? [], (v) => parseInt(v, 16)),
    'Ed25519',
    true,
    ['verify'],
  );
  const sealingKey = await crypto.subtle.generateKey({ name: 'AES-GCM', length: 256 }, false, [
    'encrypt',
    'decrypt',
  ]);
  await new Promise<void>((resolve, reject) => {
    const opening = indexedDB.open('zoen-browser-v1', 1);
    opening.onerror = () => reject(new Error('Synthetic vault open failed'));
    opening.onupgradeneeded = () => opening.result.createObjectStore('vault');
    opening.onsuccess = () => {
      const db = opening.result,
        tx = db.transaction('vault', 'readwrite');
      tx.objectStore('vault').add({ privateKey, publicKey, sealingKey }, 'keys');
      tx.oncomplete = () => {
        db.close();
        resolve();
      };
      tx.onabort = () => {
        db.close();
        reject(new Error('Synthetic vault seed failed'));
      };
    };
  });
}
async function run() {
  check(crossOriginIsolated, 'The real signing bridge requires isolation');
  const fixture = await (await fetch('/fixture.json')).json();
  check(fixture.format === 'zoen-synthetic-browser-cursor-fixture-v1', 'Not the synthetic fixture');
  phase = 'vault_seed';
  await seedSyntheticVault(fixture.account.device);
  let vault = await BrowserVault.open();
  check((await vault.device()) === fixture.account.device, 'Synthetic signer mismatch');
  await vault.save({
    kind: 'linked',
    account: { ...fixture.account, relay: location.origin },
    snapshot: fixture.snapshot,
  });
  phase = 'worker_restore';
  let core = await CoreWorker.create(vault, FIXTURE_WORKER_INTEGRITY, 'en');
  let state: MessagesState | undefined;
  const currentState = (): MessagesState | undefined => state;
  let messages: Messages | undefined;
  let reconciliations = 0,
    decodedPongs = 0,
    completedSyncs = 0;
  const syncSnapshots = new Set<string>();
  const localText = 'Synthetic retained outbox during cursor repair';
  let firstAdmission: number | undefined;
  let armReadHold = false,
    holdNextSave = false,
    saveHeld = false;
  let releaseSave = () => {};
  const heldSave = new Promise<void>((resolve) => {
    releaseSave = resolve;
  });
  const make = async () => {
    const session = await vault.load();
    check(session.kind === 'linked', 'Durable linked fixture missing');
    messages = new Messages({
      core: {
        text: async (operation) => {
          if (operation.kind === 'reconcile') ++reconciliations;
          const value = await core.text(operation);
          if (operation.kind === 'decode_server' && JSON.parse(value).kind === 'pong')
            ++decodedPongs;
          if (operation.kind === 'receive') {
            const update = JSON.parse(value);
            if (
              operation.bytes.length === fixture.sync_done.length &&
              operation.bytes.every((byte, i) => byte === fixture.sync_done[i])
            )
              syncSnapshots.add(update.snapshot);
            const metadata = update.cursor_repair;
            if (firstAdmission === undefined && typeof metadata?.admission_ms === 'number')
              firstAdmission = metadata.admission_ms;
          }
          if (operation.kind === 'queue_message' && armReadHold) {
            armReadHold = false;
            holdNextSave = true;
          }
          return value;
        },
        bytes: (operation) => core.bytes(operation),
        close: () => core.close(),
      },
      vault: {
        sign: (bytes) => vault.sign(bytes),
        save: async (session) => {
          if (holdNextSave) {
            holdNextSave = false;
            saveHeld = true;
            await heldSave;
          }
          await vault.save(session);
          if (session.kind === 'linked' && syncSnapshots.delete(session.snapshot)) ++completedSyncs;
        },
      },
      session: session as Extract<StoredSession, { kind: 'linked' }>,
      changed: (value) => {
        state = value;
      },
    });
    await messages.start();
  };
  const verified = () =>
    state?.kind === 'active' &&
    state.connection.kind === 'ready' &&
    state.conversations.length === 1 &&
    state.conversations[0]?.messages.filter((message) => message.id === 'final-retained-message')
      .length === 1;
  try {
    await make();
    phase = 'periodic_repair';
    await until(() => firstAdmission !== undefined, 10_000);
    const due = firstAdmission!;
    await until(() => performance.now() >= due - 1_000, 65_000);
    armReadHold = true;
    const write = messages!.send({ space: fixture.space, text: localText });
    void write.catch(() => {});
    await until(() => saveHeld, 1_000);
    check(performance.now() < due, 'The synthetic write must block before repair admission');
    await until(() => decodedPongs >= 2 && performance.now() >= due + 2_000, 10_000);
    check(reconciliations === 0, 'The blocked serial must retain the admitted repair');
    await until(() => performance.now() >= due + 5_000, 5_000);
    releaseSave();
    await write;
    // Real monotonic time and the unchanged60s production policy; no timer/clock injection.
    await until(verified, 80_000);
    check(state?.kind === 'active', 'Missing active projection');
    if (state?.kind !== 'active') throw new Error('Missing active projection');
    const first = state.conversations[0]!.messages.find(
      (message) => message.id === 'final-retained-message',
    )!;
    check(
      first.id === 'final-retained-message' &&
        first.text === fixture.expected_text &&
        first.delivery === 'confirmed',
      'Wrong decrypted event',
    );
    check(reconciliations >= 1, 'The final event must arrive through periodic reconciliation');
    const initial = await (await fetch('/metrics')).json();
    check(
      initial.connections === 1 && initial.authenticated === 1 && initial.fullSync.length === 2,
      'A later event/reconnect cannot stand in for repair',
    );
    check(
      initial.fullSync[0].cursor === 3 && initial.fullSync[1].cursor === 3,
      'Repair must use the durable cursor',
    );
    check(
      initial.fullSync[1].elapsedMs >= 60_000 && initial.pongs >= 2 && initial.delayedPong === true,
      'Healthy Pong stream did not span the real interval',
    );
    messages!.stop();
    core.close();
    vault.close();
    phase = 'cold_reopen';
    vault = await BrowserVault.open();
    const retained = await vault.load();
    check(
      retained.kind === 'linked' && retained.snapshot !== fixture.snapshot,
      'The verified cursor was not durably retained',
    );
    core = await CoreWorker.create(vault, FIXTURE_WORKER_INTEGRITY, 'en');
    state = undefined;
    await make();
    await until(() => verified() && completedSyncs >= 3, 10_000);
    await until(() => state?.kind === 'active' && state.connection.kind === 'ready', 1_000);
    const metrics = await (await fetch('/metrics')).json();
    check(
      metrics.connections === 2 &&
        metrics.authenticated === 2 &&
        metrics.fullSync.length === 3 &&
        metrics.fullSync[2].cursor === 4,
      'Cold restart must recover the exact accepted cursor',
    );
    const coldState = currentState();
    check(
      coldState?.kind === 'active' &&
        coldState.conversations[0]?.messages.filter(
          (message) =>
            message.id === 'final-retained-message' && message.text === fixture.expected_text,
        ).length === 1 &&
        coldState.conversations[0]?.messages.filter((message) => message.text === localText)
          .length === 1 &&
        coldState.conversations[0]?.messages.length === 2,
      'Duplicate replay changed the exact accepted event',
    );
    check(
      metrics.publicationCopies === 2 && metrics.publicationBytesIdentical,
      'Cold reopen must retain the exact pending ciphertext',
    );
    return {
      status: 'passed',
      realWorkerWasm: true,
      realWebSocket: true,
      realEncryptedIndexedDB: true,
      initialCursor: 3,
      finalCursor: 4,
      reconciliations,
      blockedSaveReleasedWithinBudget: true,
      delayedPongAfterQueuedTick: true,
      healthyConnectionsBeforeCold: 1,
      verifiedMessagesAfterCold: 1,
      retainedOutboxMessagesAfterCold: 1,
      samePendingCiphertextAfterCold: true,
      pongs: metrics.pongs,
      firstRepairElapsedMs: metrics.fullSync[1].elapsedMs,
    };
  } finally {
    releaseSave();
    messages?.stop();
    core.close();
    vault.close();
  }
}
run().then(
  (result) => {
    window.cursorResult = result;
  },
  (error: unknown) => {
    window.cursorResult = {
      status: 'failed',
      phase,
      category: 'synthetic_browser_contract',
      reason: error instanceof ContractFailure ? error.message : 'core_or_transport',
    };
  },
);
