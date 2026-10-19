import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { Miniflare, convertV4MiniflareOptions } from 'miniflare';

const moduleRoot = fileURLToPath(new URL('../', import.meta.url));
const native = JSON.parse(await readFile(process.env.ZOEN_DIRECTORY_FIXTURE
  ? resolve(process.env.ZOEN_DIRECTORY_FIXTURE) : join(moduleRoot, 'generated/directory-fixture.json'), 'utf8'));
assert.equal(native.schema, 1);
assert.equal(native.steps.length, 71);
const binary = value => Buffer.from(value, 'base64');
const named = label => {
  const value = native.steps.find(step => step.label === label);
  assert.ok(value, `Required native socket scenario: ${label}`);
  return value;
};
const transport = name => {
  const value = native.transport?.[name];
  assert.equal(typeof value, 'string', `Required native transport bytes: ${name}`);
  return binary(value);
};
function runtime(production = false) {
  return new Miniflare(convertV4MiniflareOptions({
    modulesRoot: moduleRoot,
    modules: [production ? 'src/directory-index.mjs' : 'tests/directory-fixture.mjs',
      'src/directory-state.mjs', 'generated/wasm/admission.js']
      .map(path => ({ type: 'ESModule', path: join(moduleRoot, path) }))
      .concat({ type: 'CompiledWasm', path: join(moduleRoot, 'generated/wasm/admission_bg.wasm') }),
    compatibilityDate: '2026-10-18',
    host: '127.0.0.1', port: 0,
    bindings: { RELAY_NAME: 'zoen-cloudflare-private', DIRECTORY_CELL: 'private-native-fixture' },
    durableObjects: { DIRECTORY: { className: 'DirectoryState', useSQLite: true } },
  }));
}
async function call(mf, path, input = {}) {
  const response = await mf.dispatchFetch(`https://directory-socket.test${path}`, {
    method: 'POST', body: JSON.stringify(input),
  });
  return { status: response.status, body: await response.json() };
}
async function before(mf, label) {
  const end = native.steps.findIndex(step => step.label === label);
  assert.ok(end >= 0);
  for (const step of native.steps.slice(0, end)) {
    const result = await call(mf, '/step', step);
    assert.equal(result.status, step.expected.kind === 'rejected' ? 400 : 200,
      `Native setup step ${step.label}: ${JSON.stringify(result.body)}`);
  }
}
function inbox(socket) {
  const messages = [], waiters = [];
  let close;
  const closed = new Promise(resolveClose => {
    socket.addEventListener('close', event => {
      close = { code: event.code, reason: event.reason };
      resolveClose(close);
      for (const waiter of waiters.splice(0)) waiter.reject(new Error('Socket closed before next binary frame'));
    });
  });
  socket.addEventListener('message', event => {
    assert.notEqual(typeof event.data, 'string', 'Native protocol emits binary frames only');
    const data = Buffer.from(event.data);
    const waiter = waiters.shift();
    if (waiter) waiter.resolve(data); else messages.push(data);
  });
  const bounded = (promise, label) => {
    let timer;
    return Promise.race([promise, new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Error(label)), 5000);
    })]).finally(() => clearTimeout(timer));
  };
  return {
    socket, messages,
    next() {
      if (messages.length) return Promise.resolve(messages.shift());
      if (close) return Promise.reject(new Error('Socket is already closed'));
      return bounded(new Promise((resolve, reject) => waiters.push({ resolve, reject })), 'Expected native binary frame');
    },
    closed: () => bounded(closed, 'Expected the actual socket to close'),
  };
}
async function open(mf, login) {
  assert.equal((await call(mf, '/socket-config', login)).status, 200);
  const url = new URL('/socket', await mf.ready);
  url.protocol = 'ws:';
  const socket = new WebSocket(url);
  socket.binaryType = 'arraybuffer';
  const channel = inbox(socket);
  await new Promise((accept, reject) => {
    const cleanup = () => {
      clearTimeout(timer);
      socket.removeEventListener('open', opened);
      socket.removeEventListener('error', failed);
    };
    const opened = () => { cleanup(); accept(); };
    const failed = () => { cleanup(); reject(new Error('Owned loopback socket failed to open')); };
    const timer = setTimeout(() => { cleanup(); reject(new Error('Expected the owned loopback socket to open')); }, 5000);
    socket.addEventListener('open', opened);
    socket.addEventListener('error', failed);
  });
  return channel;
}
async function hello(channel, login) {
  channel.socket.send(binary(login.hello_base64));
  assert.deepEqual(await channel.next(), binary(login.challenge_base64), 'Exact native Challenge bytes');
}
async function connect(mf, login) {
  const channel = await open(mf, login);
  await hello(channel, login);
  channel.socket.send(binary(login.auth_base64));
  assert.deepEqual(await channel.next(), binary(login.ready_base64), 'Exact native Ready bytes');
  return channel;
}
const emptySocketState = { connections: [], challenges: 0, sessions: 0 };

test('actual private WebSocket exchanges native Challenge, Ready, Register, large-u64 Res and Pong bytes', async () => {
  const mf = runtime();
  try {
    const channel = await connect(mf, named('login-first'));
    const register = named('register-first');
    channel.socket.send(binary(register.frame_base64));
    assert.deepEqual(await channel.next(), binary(register.expected.reply_base64));
    channel.socket.send(transport('large_request_base64'));
    assert.deepEqual(await channel.next(), transport('large_reply_base64'), 'u64 request ID survives the real socket exactly');
    const lifetime = (await call(mf, '/socket-state')).body.result;
    channel.socket.send(binary(named('login-first').auth_base64));
    assert.deepEqual(await channel.next(), transport('already_logged_in_error_base64'));
    channel.socket.send(transport('ping_base64'));
    assert.deepEqual(await channel.next(), transport('pong_base64'));
    assert.deepEqual((await call(mf, '/socket-state')).body.result, lifetime, 'Repeated Auth cannot renew the authenticated lifetime');
    const attachments = await call(mf, '/socket-attachments');
    assert.deepEqual(attachments.body.result, [{ keys: ['connection_id', 'version'], version: 1, opaqueConnectionId: true }]);
    channel.socket.close();
  } finally { await mf.dispose(); }
});

test('two actual preopened sockets preserve native first-registration conflict and update only the winning session', async () => {
  const mf = runtime();
  try {
    const first = await connect(mf, named('login-first'));
    const stale = await connect(mf, named('login-preopened-same-device'));
    const lifetime = (await call(mf, '/socket-state')).body.result;
    const register = named('register-first');
    first.socket.send(binary(register.frame_base64));
    assert.deepEqual(await first.next(), binary(register.expected.reply_base64));
    const conflict = named('register-stale');
    assert.equal(typeof conflict.refused_reply_base64, 'string');
    stale.socket.send(binary(conflict.frame_base64));
    assert.deepEqual(await stale.next(), binary(conflict.refused_reply_base64));
    assert.deepEqual((await call(mf, '/inspect')).body.result, register.state_after);
    assert.deepEqual((await call(mf, '/sessions')).body.result.map(session => session.registered).sort(), [false, true]);
    assert.deepEqual((await call(mf, '/socket-state')).body.result, lifetime);
    for (const channel of [first, stale]) {
      channel.socket.send(transport('ping_base64'));
      assert.deepEqual(await channel.next(), transport('pong_base64'));
      channel.socket.close();
    }
  } finally { await mf.dispose(); }
});

test('a native unsupported Req returns its exact Res error and unsupported Space traffic fails closed', async () => {
  const mf = runtime();
  try {
    await before(mf, 'login-registered-active-device');
    const channel = await connect(mf, named('login-registered-active-device'));
    const state = await call(mf, '/inspect');
    channel.socket.send(transport('unsupported_request_base64'));
    assert.deepEqual(await channel.next(), transport('unsupported_reply_base64'));
    assert.deepEqual(await call(mf, '/inspect'), state);
    channel.socket.send(transport('unsupported_space_base64'));
    assert.deepEqual(await channel.next(), transport('unsupported_space_error_base64'));
    assert.deepEqual(await call(mf, '/inspect'), state);
    channel.socket.close();
  } finally { await mf.dispose(); }
});

test('malformed Auth emits the native expected-auth Error and cannot reach Ready', async () => {
  const mf = runtime();
  try {
    const login = named('login-first'); const channel = await open(mf, login);
    await hello(channel, login);
    channel.socket.send(transport('malformed_auth_base64'));
    assert.deepEqual(await channel.next(), transport('malformed_auth_error_base64'));
    await channel.closed();
    assert.deepEqual(channel.messages, []);
    const state = (await call(mf, '/inspect')).body.result;
    assert.deepEqual(state.identities, []); assert.deepEqual(state.devices, []);
    assert.deepEqual((await call(mf, '/sessions')).body.result, []);
  } finally { await mf.dispose(); }
});

test('current certificate mismatch cannot authenticate a ready socket', async () => {
  const mf = runtime();
  try {
    await before(mf, 'login-known-device-uppercase-certificate');
    const login = named('login-known-device-uppercase-certificate');
    const channel = await open(mf, login); await hello(channel, login);
    channel.socket.send(binary(login.auth_base64));
    // Native handshake rejects this cryptographically valid but nonmatching
    // stored certificate. The transport fixture supplies its exact error bytes.
    assert.deepEqual(await channel.next(), transport('certificate_mismatch_error_base64'));
    await channel.closed();
    assert.deepEqual(channel.messages, []);
  } finally { await mf.dispose(); }
});

test('revocation silently closes an existing socket and refuses captured responses awaiting delivery', async () => {
  const mf = runtime();
  try {
    await before(mf, 'unlink-sponsored-target');
    const login = named('login-sponsored-target'); const channel = await connect(mf, login);
    const destination = { identity: login.expected.identity, device: login.expected.device };
    const frame = native.transport.pong_base64;
    const delivered = await call(mf, '/socket-deliver', { ...destination, frame_base64: frame });
    assert.equal(delivered.body.result.delivered, 1);
    assert.deepEqual(await channel.next(), binary(frame));
    assert.equal((await call(mf, '/step', named('unlink-sponsored-target'))).status, 200);
    assert.equal((await channel.closed()).code, 1008, 'Unlink must proactively close the socket before another delivery attempt');
    const refused = await call(mf, '/socket-deliver', { ...destination,
      frame_base64: named('kp-claim-first').expected.reply_base64 });
    assert.equal(refused.body.result.delivered, 0);
    assert.deepEqual(channel.messages, [], 'No queued response may cross the final revocation fence');
  } finally { await mf.dispose(); }
});

test('self-Unlink commits revocation and closes without delivering the stale success reply', async () => {
  const mf = runtime();
  try {
    await before(mf, 'kp-unlink-caller');
    const channel = await connect(mf, named('kp-login-active-source'));
    channel.socket.send(binary(named('kp-unlink-caller').frame_base64));
    assert.equal((await channel.closed()).code, 1008);
    assert.deepEqual(channel.messages, [], 'The native ResDone must not bypass final durable authorization');
    const state = (await call(mf, '/inspect')).body.result;
    const source = named('kp-login-active-source').expected;
    assert.equal(state.devices.find(row => row.identity === source.identity && row.device === source.device).revoked, true);
  } finally { await mf.dispose(); }
});

test('actual client disconnect promptly deletes its authenticated connection and owned session', async () => {
  const mf = runtime();
  try {
    const channel = await connect(mf, named('login-first'));
    assert.equal((await call(mf, '/socket-state')).body.result.connections.length, 1);
    channel.socket.close(1000, 'synthetic client disconnect');
    assert.equal((await channel.closed()).code, 1000);
    assert.deepEqual(channel.messages, []);
    assert.deepEqual((await call(mf, '/socket-state')).body.result, emptySocketState);
  } finally { await mf.dispose(); }
});

for (const stage of ['hello', 'auth']) {
  test(`${stage === 'hello' ? 'Hello' : 'Auth'} idle deadline closes the actual socket and deletes its owned durable records`, async () => {
    const mf = runtime();
    try {
      const login = named('login-first');
      const channel = await open(mf, login);
      if (stage === 'auth') await hello(channel, login);
      const pending = (await call(mf, '/socket-state')).body.result;
      assert.equal(pending.connections.length, 1);
      assert.equal(pending.connections[0].stage, stage);
      assert.equal(pending.challenges, stage === 'auth' ? 1 : 0);
      const alarm = await call(mf, '/socket-alarm', { now_ms: String(Number(login.now_ms) + 10_000) });
      assert.equal(alarm.status, 200);
      assert.deepEqual(await channel.next(), transport(`expired_${stage}_error_base64`));
      const closed = await channel.closed().catch(failure => {
        throw new Error(`${failure.message}; sanitized server alarm: ${JSON.stringify(alarm.body.result)}`);
      });
      assert.equal(closed.code, 1008);
      assert.deepEqual(channel.messages, []);
      assert.deepEqual((await call(mf, '/socket-state')).body.result, emptySocketState);
    } finally { await mf.dispose(); }
  });
}

test('fresh production-handler reconstruction keeps the accepted socket and original authenticated deadline', async () => {
  const mf = runtime();
  try {
    const login = named('login-first'); const channel = await connect(mf, login);
    const original = (await call(mf, '/socket-state')).body.result;
    const attachments = (await call(mf, '/socket-attachments')).body.result;
    const deadline = String(Number(login.now_ms) + 900_000);
    assert.deepEqual(original, { connections: [{ stage: 'ready', expires_ms: deadline,
      challenge: false, session: true }], challenges: 0, sessions: 1 });
    const restored = await call(mf, '/socket-reconstruct', {
      now_ms: String(Number(deadline) - 1), frame_base64: native.transport.ping_base64,
    });
    assert.equal(restored.status, 200);
    assert.deepEqual(restored.body.result, { attachmentUnchanged: true });
    assert.deepEqual(await channel.next(), transport('pong_base64'));
    assert.deepEqual((await call(mf, '/socket-state')).body.result, original, 'Activity must not renew the stored session or connection deadline');
    assert.deepEqual((await call(mf, '/socket-attachments')).body.result, attachments);
    assert.equal((await call(mf, '/socket-reconstruct', { now_ms: deadline, alarm: true })).status, 200);
    assert.equal((await channel.closed()).code, 1008);
    assert.deepEqual(channel.messages, [], 'No authenticated payload may follow session expiry');
    assert.deepEqual((await call(mf, '/socket-state')).body.result, emptySocketState);
  } finally { await mf.dispose(); }
});

test('expired idle sockets actually close and release the bounded 256-connection capacity', async () => {
  const mf = runtime();
  try {
    const login = named('login-first');
    const channels = [];
    for (let index = 0; index < 256; index++) channels.push(await open(mf, login));
    const full = (await call(mf, '/socket-state')).body.result;
    assert.equal(full.connections.length, 256);
    const refused = await mf.dispatchFetch('https://directory-socket.test/socket', { headers: { Upgrade: 'websocket' } });
    assert.equal(refused.status, 429); assert.equal(refused.webSocket, null);
    assert.deepEqual((await call(mf, '/socket-state')).body.result, full, 'Capacity refusal must not allocate another durable connection');
    const alarm = await call(mf, '/socket-alarm', { now_ms: String(Number(login.now_ms) + 10_000) });
    assert.equal(alarm.status, 200);
    for (const channel of channels) {
      assert.deepEqual(await channel.next(), transport('expired_hello_error_base64'));
      const closed = await channel.closed().catch(failure => {
        throw new Error(`${failure.message}; sanitized server alarm: ${JSON.stringify(alarm.body.result)}`);
      });
      assert.equal(closed.code, 1008);
    }
    assert.deepEqual((await call(mf, '/socket-state')).body.result, emptySocketState);
    const replacement = await connect(mf, login);
    replacement.socket.send(transport('ping_base64'));
    assert.deepEqual(await replacement.next(), transport('pong_base64'));
    replacement.socket.close();
  } finally { await mf.dispose(); }
});

test('text, malformed binary and oversized messages close without directory mutation or leaked sessions', async () => {
  const mf = runtime();
  try {
    const login = named('login-first');
    for (const payload of ['not native binary', new Uint8Array([0]), new Uint8Array(1024 * 1024 + 1)]) {
      const channel = await connect(mf, login);
      const state = await call(mf, '/inspect');
      channel.socket.send(payload);
      assert.equal((await channel.closed()).code, 1003);
      assert.deepEqual(channel.messages, []);
      assert.deepEqual(await call(mf, '/inspect'), state);
      assert.deepEqual((await call(mf, '/socket-state')).body.result, emptySocketState);
    }
  } finally { await mf.dispose(); }
});

test('trusted delivery rejects handshake injection before sending while valid native traffic remains usable', async () => {
  const mf = runtime();
  try {
    const login = named('login-first'); const channel = await connect(mf, login);
    const destination = { identity: login.expected.identity, device: login.expected.device };
    const state = (await call(mf, '/socket-state')).body.result;
    for (const frame_base64 of [login.ready_base64, login.challenge_base64, Buffer.from([0]).toString('base64')]) {
      assert.equal((await call(mf, '/socket-deliver', { ...destination, frame_base64 })).status, 400);
      assert.deepEqual((await call(mf, '/socket-state')).body.result, state);
    }
    channel.socket.send(transport('ping_base64'));
    assert.deepEqual(await channel.next(), transport('pong_base64'));
    assert.deepEqual(channel.messages, []);
    channel.socket.close();
  } finally { await mf.dispose(); }
});

test('late Hello and Auth stage SQL failures roll back challenge/session creation and close without leaked capabilities', async () => {
  const mf = runtime();
  try {
    const login = named('login-first');
    const state = await call(mf, '/inspect');
    for (const stage of ['auth', 'ready']) {
      const channel = await open(mf, login);
      if (stage === 'ready') await hello(channel, login);
      assert.equal((await call(mf, '/reject', { table: `socket-${stage}` })).status, 200);
      channel.socket.send(binary(stage === 'auth' ? login.hello_base64 : login.auth_base64));
      assert.deepEqual(await channel.next(), transport('storage_error_base64'));
      assert.equal((await channel.closed()).code, 1008);
      assert.deepEqual(channel.messages, [], 'Failed stage transitions cannot disclose a Challenge or Ready');
      assert.deepEqual((await call(mf, '/socket-state')).body.result, emptySocketState);
      assert.deepEqual(await call(mf, '/inspect'), state);
      assert.equal((await call(mf, '/allow')).status, 200);
    }
  } finally { await mf.dispose(); }
});

test('production outer HTTP stays404 even for native WebSocket upgrade paths', async () => {
  const mf = runtime(true);
  try {
    for (const path of ['/', '/v1/sync', '/sync', '/socket']) {
      const response = await mf.dispatchFetch('https://directory-private.test' + path,
        { headers: { Upgrade: 'websocket' } });
      assert.equal(response.status, 404); assert.equal(response.webSocket, null);
    }
  } finally { await mf.dispose(); }
});
