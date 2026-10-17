import assert from 'node:assert/strict';
import { generateKeyPairSync, sign, createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
if (args.length !== 2 || args[0] !== '--relay')
  throw new Error(
    'Usage: node scripts/qa-web-primary-native-history.mjs --relay http://127.0.0.1:8790',
  );
const origin = new URL(args[1]);
if (
  origin.protocol !== 'http:' ||
  !['127.0.0.1', '[::1]'].includes(origin.hostname) ||
  origin.username ||
  origin.password ||
  origin.pathname !== '/' ||
  origin.search ||
  origin.hash
)
  throw new Error(
    'This synthetic QA requires a credential-free literal loopback HTTP relay origin.',
  );
const relay = origin.origin;
const { Messages } = await import(pathToFileURL(`${root}/packages/zoen-ui/web/messages.ts`).href);
const { AuthenticatedTransport } = await import(
  pathToFileURL(`${root}/packages/zoen-ui/web/transport.ts`).href
);
const { sealSession, openSession } = await import(
  pathToFileURL(`${root}/packages/zoen-ui/web/vault.ts`).href
);
const { sealAuthority, openAuthority } = await import(
  pathToFileURL(`${root}/packages/zoen-ui/web/authority.ts`).href
);
const wasmBytes = await readFile(`${root}/apps/zoen/public/core/zoen_web_core_bg.wasm`);
let wasm = await import(
  pathToFileURL(`${root}/apps/zoen/modules/zoen-core/generated/web/zoen_web_core.js`).href +
    '?qa=first'
);
await wasm.default({ module_or_path: wasmBytes });
const signing = generateKeyPairSync('ed25519'),
  device = signing.publicKey.export({ format: 'der', type: 'spki' }).subarray(-32).toString('hex');
globalThis.zoenSign = (payload) => new Uint8Array(sign(null, payload, signing.privateKey));
const encryptionKey = await crypto.subtle.generateKey({ name: 'AES-GCM', length: 256 }, false, [
  'encrypt',
  'decrypt',
]);
let primary = new wasm.BrowserPrimary(device, relay, 'QA reverse browser history');
const account = JSON.parse(primary.account());
account.handle = `qa_rev_${Date.now().toString(36)}`;
const authority = await sealAuthority(
  primary.authority_bytes(),
  encryptionKey,
  account.identity,
  device,
);
let encrypted = await sealSession({ kind: 'linked', account, snapshot: '' }, encryptionKey),
  client = null,
  messages = null,
  state = null,
  saves = 0,
  child = null,
  registrationTransport = null;
let checkpointResolve;
const checkpoint = new Promise((resolve) => {
  checkpointResolve = resolve;
});
let checkpointSeen = false;
const vault = {
  device: async () => device,
  sign: async (payload) => new Uint8Array(sign(null, payload, signing.privateKey)),
  save: async (value) => {
    encrypted = await sealSession(value, encryptionKey);
    saves++;
    if (value.kind === 'linked' && value.snapshot) {
      const sender = JSON.parse(value.snapshot).linking?.sending;
      if (sender?.uploaded === 1 && !checkpointSeen) {
        checkpointSeen = true;
        checkpointResolve(structuredClone(encrypted));
      }
    }
  },
};
const core = {
  close() {
    client?.free();
    client = null;
  },
  async text(op) {
    switch (op.kind) {
      case 'decode_server':
        return wasm.decode_server(op.bytes);
      case 'restore':
        client?.free();
        client = new wasm.BrowserCore(op.account, op.snapshot);
        return client.view();
      case 'claim_clock':
        return client.claim_clock(op.value);
      case 'bootstrap':
        return client.bootstrap();
      case 'receive':
        return client.receive(op.bytes, op.now);
      case 'maintenance':
        return client.maintenance(op.now);
      case 'membership_request':
        return client.membership_request(op.request, op.now);
      case 'link_prepare':
        return client.link_prepare(primary, op.code, op.now);
      case 'link_request':
        return client.link_request(op.request, op.now);
      case 'queue_post':
        return client.queue_post(
          op.space,
          op.text,
          op.replyTo || undefined,
          op.thread,
          op.forwarded,
          op.now,
        );
      case 'document_uploads':
        return client.document_uploads();
      case 'document_uploaded':
        return client.document_uploaded(op.blobs);
      case 'history_status':
        return client.history_status();
      default:
        throw new Error('Unexpected core text operation ' + op.kind);
    }
  },
  async bytes(op) {
    switch (op.kind) {
      case 'hello':
        return wasm.hello(op.identity, op.device, op.cert);
      case 'auth_message':
        return wasm.auth_message(op.nonce, op.relay);
      case 'device_cert_message':
        return wasm.device_cert_message(op.device);
      case 'auth':
        return wasm.auth(op.signature);
      case 'ping':
        return wasm.ping();
      case 'request':
        return wasm.request(op.requestId, op.op, op.value);
      case 'local_chunk':
        return client.local_chunk(op.blob);
      case 'blob_put_message':
        return wasm.blob_put_message(op.sha, op.ts, op.relay);
      default:
        throw new Error('Unexpected core bytes operation ' + op.kind);
    }
  },
};
async function withDeadline(promise, label, ms) {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error(label)), ms);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}
async function until(predicate, label, ms = 90000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    const result = await predicate();
    if (result) return result;
    if (state?.kind === 'failed') throw new Error(state.message);
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error('Timed out ' + label);
}
const actualFetch = globalThis.fetch;
let interrupted = true;
const puts = [];
globalThis.fetch = async (input, options) => {
  const url = String(input);
  if (options?.method === 'PUT' && url.startsWith(relay + '/v1/transfer/')) {
    const index = Number(url.split('/').at(-1)),
      digest = createHash('sha256').update(options.body).digest('hex');
    puts.push({ index, digest, bytes: options.body.length, afterRestart: !interrupted });
    if (interrupted && index >= 1)
      throw new Error('QA synthetic connection interruption after first durable chunk');
  }
  return actualFetch(input, options);
};
const replies = [];
let pendingResolve, readyResolve, readyReject;
const pending = new Promise((resolve) => {
    pendingResolve = resolve;
  }),
  ready = new Promise((resolve, reject) => {
    readyResolve = resolve;
    readyReject = reject;
  });
function nativeCommand(value) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Native command deadline ' + value.op)), 45000);
    replies.push({ resolve, reject, timer });
    child.stdin.write(JSON.stringify(value) + '\n');
  });
}
function startNative() {
  child = spawn(
    `${root}/target/debug/examples/qa-device`,
    [
      '--handle',
      `qa_dest_${Date.now().toString(36)}`,
      '--relay',
      relay,
      '--mode',
      'linked',
      '--expected-identity',
      account.identity,
    ],
    { stdio: ['pipe', 'pipe', 'pipe'], env: { ...process.env, ZOEN_TRANSFER_STOP_AFTER: '1' } },
  );
  const lines = createInterface({ input: child.stdout });
  let stderr = '';
  child.stderr.on('data', (x) => {
    stderr = (stderr + x).slice(-300);
  });
  lines.on('line', (line) => {
    const value = JSON.parse(line);
    if (value.event === 'pending') {
      pendingResolve(value.request);
      return;
    }
    if (value.event === 'ready') {
      readyResolve(value);
      return;
    }
    const next = replies.shift();
    if (next) {
      clearTimeout(next.timer);
      if (value.ok) next.resolve(value.result);
      else next.reject(new Error('Canonical native QA operation refused'));
    }
  });
  child.once('error', readyReject);
  child.once('exit', () => {
    readyReject(new Error('Native fixture closed ' + stderr));
    for (const next of replies) {
      clearTimeout(next.timer);
      next.reject(new Error('Native fixture closed'));
    }
  });
}
async function openMessages() {
  messages = new Messages({
    core,
    vault,
    session: await openSession(encrypted, encryptionKey),
    changed: (next) => {
      state = next;
      if (next.kind === 'failed') console.error('QA state failed:', next.message);
    },
  });
  await messages.start();
  await until(() => state?.kind === 'active' && state.connection.kind === 'ready', 'Web ready');
}
try {
  // Register a fresh, synthetic person using the same certified-device protocol.
  let registrationResolve, registrationReject;
  const registration = new Promise((resolve, reject) => {
    registrationResolve = resolve;
    registrationReject = reject;
  });
  registrationTransport = new AuthenticatedTransport(
    core,
    vault,
    account,
    (next) => {
      if (next.kind === 'failed') registrationReject(new Error(next.message));
    },
    async (frame) => {
      if (frame.kind === 'ready') {
        try {
          const result = await registrationTransport.request(
            'register',
            JSON.stringify({
              id: account.identity,
              kind: 'Person',
              name: '',
              handle: account.handle,
              tint_hex: '#4F7CFF',
              glyph: null,
              owner: null,
              bio: '',
              owner_proof: null,
            }),
          );
          assert.equal(result.kind, 'registered');
          registrationResolve();
        } catch (error) {
          registrationReject(error);
        }
      }
    },
  );
  registrationTransport.start();
  await withDeadline(registration, 'Synthetic registration deadline', 30000);
  registrationTransport.stop();
  await openMessages();
  const space = await until(async () => {
    try {
      return await messages.membershipRequest({
        op: 'create_group',
        title: 'QA Web primary → native multi-chunk',
        member_ids: [],
      });
    } catch (error) {
      if (/Conecte ao servidor/.test(error.message)) return false;
      throw error;
    }
  }, 'initial SyncDone');
  await until(
    () => state.conversations.some((chat) => chat.id === space && chat.can_send),
    'new encrypted group',
  );
  const count = 90,
    texts = [];
  for (let index = 0; index < count; index++) {
    const text = `QA reverse ${index.toString().padStart(3, '0')} ${'synthetic_history_'.repeat(1650)}`;
    texts.push(text);
    await messages.post({ space, text });
    if (index % 20 === 0) console.error('QA posted:', index + 1);
    await new Promise((r) => setTimeout(r, 125));
  }
  await until(
    () =>
      state.conversations
        .find((chat) => chat.id === space)
        ?.messages.filter((message) => message.delivery === 'confirmed').length === count,
    'all real encrypted messages ACK',
    180000,
  );
  console.error('QA phase: native pairing');
  startNative();
  const request = await withDeadline(
    Promise.race([
      pending,
      ready.then(() => {
        throw new Error('Missing native pending event');
      }),
    ]),
    'Native pairing deadline',
    30000,
  );
  assert.equal(
    await messages.linkRequest({ op: 'link_code_check', code: request.code }),
    request.check,
  );
  const linked = await messages.linkDevice(request.code);
  console.error('QA phase: native startup');
  const native = await withDeadline(ready, 'Native startup deadline', 45000);
  console.error('QA phase: native membership');
  assert.equal(native.account.identity_id, account.identity);
  assert.equal(native.account.device_id, linked.device_id);
  await until(async () => {
    const progress = await messages.linkProgress(linked.device_id);
    return progress[1] > 0 && progress[0] === progress[1];
  }, 'native Welcome and memberships');
  console.error('QA phase: initial encrypted history');
  const before = await nativeCommand({ op: 'timeline', space_id: space, before: null, limit: 150 });
  assert.equal(
    before.entries.some((entry) => JSON.stringify(entry).includes('QA reverse 000')),
    false,
  );
  const sending = messages.sendHistory(linked.device_id).catch((error) => error);
  const frozen = await withDeadline(checkpoint, 'First outgoing chunk checkpoint deadline', 60000);
  const frozenSession = await openSession(frozen, encryptionKey),
    sender = JSON.parse(frozenSession.snapshot).linking.sending;
  assert.ok(sender.chunks.length >= 3);
  assert.equal(sender.uploaded, 1);
  const retainedChunks = sender.chunks.map((chunk) =>
    createHash('sha256').update(Uint8Array.from(chunk)).digest('hex'),
  );
  messages.stop();
  messages = null;
  await sending;
  core.close();
  primary.free();
  primary = null;
  // A distinct WASM instance and restored encrypted authority/session simulate the
  // WASM runtime being destroyed. Browser-process/IndexedDB durability and UI
  // are separate acceptance checks; this runner does not render a browser.
  wasm = await import(
    pathToFileURL(`${root}/apps/zoen/modules/zoen-core/generated/web/zoen_web_core.js`).href +
      '?qa=second'
  );
  await wasm.default({ module_or_path: wasmBytes });
  const seed = await openAuthority(authority, encryptionKey, device);
  try {
    primary = wasm.BrowserPrimary.restore(seed, JSON.stringify(account), device);
  } finally {
    seed.fill(0);
  }
  console.error('QA phase: distinct WASM cold restart');
  encrypted = frozen;
  interrupted = false;
  state = null;
  await openMessages();
  const sent = await messages.sendHistory(linked.device_id);
  assert.equal(sent, count);
  const successfulPuts = puts.filter((put) => put.afterRestart);
  assert.ok(successfulPuts.length >= 2);
  assert.ok(successfulPuts.every((put) => put.digest === retainedChunks[put.index]));
  assert.equal(
    successfulPuts.some((put) => put.index === 0),
    false,
    'already acknowledged chunk must not upload again',
  );
  console.error('QA phase: interrupted native import');
  let result = null,
    interruptions = 0;
  for (let retry = 0; retry < sender.chunks.length + 2; retry++) {
    try {
      result = await nativeCommand({ op: 'receive_history', timeout_ms: 10000 });
      break;
    } catch (error) {
      if (error.message !== 'Canonical native QA operation refused') throw error;
      interruptions++;
      const status = await nativeCommand({ op: 'history_status' });
      assert.ok(status.chunks_received > 0);
    }
  }
  assert.ok(result);
  assert.equal(result.chunks, sender.chunks.length);
  assert.equal(result.messages, count);
  assert.ok(result.resumed >= 1);
  const page = await nativeCommand({ op: 'timeline', space_id: space, before: null, limit: 150 });
  const actual = page.entries
    .filter((entry) => 'Message' in entry.kind)
    .map((entry) => entry.kind.Message.text);
  assert.equal(actual.length, count);
  assert.deepEqual(actual, texts);
  assert.equal((await nativeCommand({ op: 'history_status' })).initial_pending, false);
  console.log(
    JSON.stringify({
      ok: true,
      journey:
        'actualWASM primary → native certified device → multi-chunk encrypted history → distinct senderWASM → encrypted checkpoint restart → exact retained chunks → canonical native interrupted import',
      messages: count,
      chunks: sender.chunks.length,
      native_interruptions: interruptions,
      native_resumed: result.resumed,
      encrypted_checkpoints: saves,
      browser_storage_verified: false,
      wasm_sha256: createHash('sha256').update(wasmBytes).digest('hex'),
      ui_verified: false,
    }),
  );
} catch (error) {
  console.error(error.stack ?? String(error));
  process.exitCode = 1;
} finally {
  registrationTransport?.stop();
  messages?.stop();
  core.close();
  primary?.free();
  delete globalThis.zoenSign;
  globalThis.fetch = actualFetch;
  child?.stdin.end();
  setTimeout(() => child?.kill(), 1000).unref();
}
