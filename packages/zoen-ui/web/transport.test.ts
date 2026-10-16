import test from 'node:test';
import assert from 'node:assert/strict';
import { AuthenticatedTransport, type ConnectionState } from './transport.ts';
import type { CoreOperation } from './worker-client.ts';
import { Messages, type FileDoc, type MessagesState } from './messages.ts';
import type { StoredSession } from './session.ts';

// These tests exercise connection/storage ordering. Native protobuf/MLS
// interoperability requires the separate owned native relay fixture journey.
class Socket {
  static readonly OPEN = 1;
  static readonly instances: Socket[] = [];
  readyState = 0;
  binaryType = '';
  bufferedAmount = 0;
  sent: Uint8Array[] = [];
  onopen: (() => void) | undefined;
  onmessage: ((event: { data: ArrayBuffer }) => void) | undefined;
  onclose: ((event: { code: number }) => void) | undefined;
  onerror: (() => void) | undefined;
  readonly url: string;
  constructor(url: string) { this.url = url; Socket.instances.push(this); }
  open() { this.readyState = Socket.OPEN; this.onopen?.(); }
  send(value: Uint8Array) { this.sent.push(Uint8Array.from(value)); }
  frame(value: unknown) { this.onmessage?.({ data: new TextEncoder().encode(JSON.stringify(value)).buffer }); }
  close(code = 1006) { this.readyState = 3; this.onclose?.({ code }); }
}
const login = { identity: '11'.repeat(32), device: '22'.repeat(32), cert: 'cert', relay: 'https://relay.example' };
const challenge = { kind: 'challenge', nonce: 'nonce', relay: 'relay.example', protocol: 4 };
const ready = { kind: 'ready', identity: login.identity, registered: true };
const vault = { sign: async () => new Uint8Array(64) };
function unitCodec() {
  return { bytes: async (operation: CoreOperation) => new TextEncoder().encode(operation.kind),
    text: async (operation: CoreOperation) => {
      if (operation.kind !== 'decode_server') throw new Error('Unexpected test operation.');
      return new TextDecoder().decode(operation.bytes);
    } };
}
function socket() { const value = Socket.instances.at(-1); assert.ok(value); return value; }
function install(t: test.TestContext) {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'WebSocket');
  Socket.instances.length = 0;
  Object.defineProperty(globalThis, 'WebSocket', { configurable: true, value: Socket });
  t.mock.timers.enable({ apis: ['setTimeout'] });
  t.after(() => { if (descriptor) Object.defineProperty(globalThis, 'WebSocket', descriptor); else Reflect.deleteProperty(globalThis, 'WebSocket'); });
}
async function settle() { await new Promise<void>((resolve) => setImmediate(resolve)); }
async function authenticate() {
  socket().open(); await settle(); socket().frame(challenge); await settle(); socket().frame(ready); await settle();
}

test('disconnect during Ready FetchLink preserves reconnect', async (t) => {
  install(t);
  const states: ConnectionState[] = [];
  const transport = new AuthenticatedTransport(unitCodec(), vault, login, (state) => states.push(state), async (frame) => {
    if (frame.kind === 'ready') await transport.request('fetch_link', 'box');
  });
  t.after(() => transport.stop());
  transport.start(); await authenticate();
  const first = socket(); assert.equal(first.sent.length, 3);
  first.close(); await settle();
  assert.equal(states.at(-1)?.kind, 'reconnecting');
  assert.equal(states.some((state) => state.kind === 'failed'), false);
  t.mock.timers.tick(900); await settle();
  assert.equal(Socket.instances.length, 2);
  assert.notEqual(socket(), first);
  await authenticate();
  assert.equal(states.at(-1)?.kind, 'ready');
});

test('a delayed Ready decode from a closed generation cannot authenticate its replacement', async (t) => {
  install(t);
  const codec = unitCodec();
  let finish = (_: string) => {};
  const delayed = new Promise<string>((resolve) => { finish = resolve; });
  let delay = true;
  const core = { ...codec, text: async (operation: CoreOperation) => {
    const result = await codec.text(operation);
    if (delay && JSON.parse(result).kind === 'ready') { delay = false; return delayed; }
    return result;
  } };
  const states: ConnectionState[] = [];
  let received = 0;
  const transport = new AuthenticatedTransport(core, vault, login, (state) => states.push(state), async () => { ++received; });
  t.after(() => transport.stop());
  transport.start(); await authenticate(); socket().close();
  t.mock.timers.tick(900); await settle();
  socket().open(); await settle(); socket().frame(challenge); await settle();
  finish(JSON.stringify(ready)); await settle();
  assert.equal(received, 0);
  assert.equal(states.at(-1)?.kind, 'authenticating');
  socket().frame(ready); await settle();
  assert.equal(received, 1);
  assert.equal(states.at(-1)?.kind, 'ready');
});

test('closing a generation while auth bytes await never sends stale authentication', async (t) => {
  install(t);
  const codec = unitCodec();
  let finish = (_: Uint8Array<ArrayBuffer>) => {};
  const delayed = new Promise<Uint8Array<ArrayBuffer>>((resolve) => { finish = resolve; });
  const core = { ...codec, bytes: async (operation: CoreOperation) => operation.kind === 'auth' ? delayed : codec.bytes(operation) };
  const transport = new AuthenticatedTransport(core, vault, login, () => {}, async () => {});
  t.after(() => transport.stop());
  transport.start(); socket().open(); await settle(); socket().frame(challenge); await settle();
  const first = socket(); first.close(); t.mock.timers.tick(900); await settle();
  finish(new Uint8Array([99])); await settle();
  assert.equal(first.sent.length, 1);
  assert.equal(socket().sent.length, 0);
});

test('native early presence does not replace or prevent authenticated Ready', async (t) => {
  install(t);
  const states: ConnectionState[] = [];
  const transport = new AuthenticatedTransport(unitCodec(), vault, login, (state) => states.push(state), async () => {});
  t.after(() => transport.stop());
  transport.start(); socket().open(); await settle(); socket().frame(challenge); await settle();
  socket().frame({ kind: 'other' }); await settle();
  assert.equal(states.at(-1)?.kind, 'authenticating');
  socket().frame(ready); await settle();
  assert.equal(states.at(-1)?.kind, 'ready');
});

const linked: Extract<StoredSession, { kind: 'linked' }> = { kind: 'linked', account: {
  ...login, name: 'Web fixture', handle: 'web_fixture', primary: '33'.repeat(32), primary_hpke: '', hpke_secret: '',
  link_secret: '', agreement_secret: '', profile_key: '', profile_key_version: 0, profile_seen_version: 0, profile_fields: '',
}, snapshot: '' };
const conversation = { id: 'sp_fixture', title: 'Fixture', kind: 'group', can_send: true, waiting: null, unreadable: 0, messages: [] };
function messagingCore() {
  let closed = false;
  return { ...unitCodec(), close() { closed = true; }, get closed() { return closed; },
    text: async (operation: CoreOperation) => {
      if (operation.kind === 'restore') return JSON.stringify([conversation]);
      if (operation.kind === 'decode_server') return new TextDecoder().decode(operation.bytes);
      if (operation.kind === 'bootstrap') return JSON.stringify({ snapshot: 'durable bootstrap', frames: [[7]], conversations: [conversation] });
      if (operation.kind === 'queue_message') return JSON.stringify({ snapshot: 'new MLS send ratchet', frames: [[8]], conversations: [conversation] });
      if (operation.kind === 'receive') return JSON.stringify({ snapshot: 'received', frames: [], conversations: [conversation] });
      throw new Error('Unexpected test core operation.');
    } };
}

test('production message controller terminates the core and emits nothing when IndexedDB save fails', async (t) => {
  install(t);
  const core = messagingCore();
  const states: MessagesState[] = [];
  const messages = new Messages({ core, session: linked, vault: { ...vault, save: async () => { throw new Error('QuotaExceededError'); } },
    changed: (state) => states.push(state) });
  t.after(() => messages.stop());
  await messages.start(); await authenticate();
  assert.equal(socket().sent.length, 2); // hello/auth only; no MLS/bootstrap frame.
  assert.equal(core.closed, true);
  assert.equal(states.at(-1)?.kind, 'failed');
  t.mock.timers.tick(40_000); await settle();
  assert.equal(Socket.instances.length, 1);
});

test('mutation refusals propagate in Portuguese without terminating the session; retry persists before send', async (t) => {
  install(t);
  const base = messagingCore();
  let refusal = 'Only the person who sent this message can change it.';
  let mutationSaves = 0;
  const core = { ...base, text: async (operation: CoreOperation) => {
    if (operation.kind !== 'queue_annotation') return base.text(operation);
    if (refusal) throw new Error(refusal);
    return JSON.stringify({ snapshot: 'mutation', frames: [[9]], conversations: [conversation] });
  } };
  const states: MessagesState[] = [];
  const messages = new Messages({ core, session: linked, vault: { ...vault, save: async (saved) => {
    if (saved.kind === 'linked' && saved.snapshot === 'mutation') {
      assert.equal(socket().sent.length, 3);
      ++mutationSaves;
    }
  } }, changed: (state) => states.push(state) });
  t.after(() => messages.stop());
  await messages.start(); await authenticate();
  await assert.rejects(messages.annotate('sp_fixture', 'target', { Edit: { text: 'draft' } }), /Só quem enviou/);
  assert.equal(states.at(-1)?.kind, 'active');
  assert.equal(base.closed, false);
  assert.equal(socket().sent.length, 3);
  refusal = 'Essa mensagem foi apagada.';
  await assert.rejects(messages.annotate('sp_fixture', 'target', { Edit: { text: 'draft' } }), /Essa mensagem foi apagada/);
  refusal = 'Write a message of up to 32 KB.';
  await assert.rejects(messages.annotate('sp_fixture', 'target', { Edit: { text: '' } }), /não pode ficar vazia/);
  assert.equal(states.at(-1)?.kind, 'active');
  refusal = '';
  await messages.annotate('sp_fixture', 'target', { Edit: { text: 'draft' } });
  assert.equal(mutationSaves, 1);
  assert.equal(socket().sent.length, 4);
});

test('mutation persistence failure is fatal and never emits an unsaved annotation', async (t) => {
  install(t);
  const base = messagingCore();
  const core = { ...base, text: async (operation: CoreOperation) => operation.kind === 'queue_annotation'
    ? JSON.stringify({ snapshot: 'mutation', frames: [[9]], conversations: [conversation] }) : base.text(operation) };
  const states: MessagesState[] = [];
  const messages = new Messages({ core, session: linked, vault: { ...vault, save: async (saved) => {
    if (saved.kind === 'linked' && saved.snapshot === 'mutation') throw new Error('Storage denied');
  } }, changed: (state) => states.push(state) });
  t.after(() => messages.stop());
  await messages.start(); await authenticate();
  const current = socket();
  await assert.rejects(messages.annotate('sp_fixture', 'target', 'Delete'), /Storage denied/);
  assert.equal(base.closed, true);
  assert.equal(states.at(-1)?.kind, 'failed');
  assert.equal(current.sent.length, 3);
});

test('production send persists the ratchet before emitting and reconnect replays only saved state', async (t) => {
  install(t);
  const core = messagingCore();
  let complete = () => {};
  const saving = new Promise<void>((resolve) => { complete = resolve; });
  const saves: StoredSession[] = [];
  const messages = new Messages({ core, session: linked, vault: { ...vault, save: async (session) => {
    saves.push(session);
    if (session.kind === 'linked' && session.snapshot === 'new MLS send ratchet') await saving;
  } }, changed: () => {} });
  t.after(() => messages.stop());
  await messages.start(); await authenticate();
  const first = socket(); assert.equal(first.sent.length, 3);
  const sending = messages.send({ space: 'sp_fixture', text: 'Encrypted message' }); await settle();
  assert.equal(first.sent.length, 3);
  first.close(); await settle(); complete(); await sending;
  assert.equal(first.sent.length, 3);
  t.mock.timers.tick(900); await settle(); await authenticate();
  assert.equal(socket().sent.length, 3);
  assert.equal(saves.length, 3);
  assert.equal(core.closed, false);
});

const digest = async (data: Uint8Array) => Buffer.from(await crypto.subtle.digest('SHA-256', Uint8Array.from(data))).toString('hex');
async function attachment(data = new Uint8Array([1, 2, 3])): Promise<FileDoc> {
  const sha256 = await digest(data);
  return { name: 'clip.webm', path: '', mime: 'audio/webm', bytes: data.length, sha256,
    chunks: [{ sha256, bytes: data.length, blob: '44'.repeat(32), key: '55'.repeat(32) }], thumb: null, voice: null };
}

test('native voice metadata and legacy paths survive parsing; forwarding clears only the reply', async (t) => {
  install(t);
  const file = { ...await attachment(), path: 'VoiceNotes/native.webm',
    voice: { ms: 3400, levels: '08ff80', transcript: 'Bring the tent', reply: { to: 'source-entry', thread: true } } };
  const { voice: _, ...legacy } = file;
  const entry = { id: 'voice', entry: 'ordered', author: login.identity, name: 'Member', text: '',
    at_ms: 1, delivery: 'confirmed', failure: null };
  const received = [{ ...conversation, messages: [
    { ...entry, file }, { ...entry, id: 'legacy', file: legacy },
    { ...entry, id: 'sparse', file: { ...file, voice: { ms: 1000, reply: { to: 'quote' } } } },
  ] }];
  const states: MessagesState[] = [];
  const queued: FileDoc[] = [];
  const codec = messagingCore();
  const core = { ...codec, text: async (op: CoreOperation) => {
    if (op.kind === 'restore') return JSON.stringify(received);
    if (op.kind === 'bootstrap' || op.kind === 'queue_file') {
      if (op.kind === 'queue_file') queued.push(JSON.parse(op.doc));
      return JSON.stringify({ snapshot: 'native voice', frames: [], conversations: received });
    }
    return codec.text(op);
  } };
  const messages = new Messages({ core, session: linked, vault: { ...vault, save: async () => {} }, changed: (state) => states.push(state) });
  t.after(() => messages.stop());
  await messages.start(); await authenticate();
  const state = states.at(-1);
  if (state?.kind !== 'active') assert.fail('message controller failed');
  const files = state.conversations[0]!.messages.map((m) => m.file);
  assert.deepEqual(files[0], file);
  assert.deepEqual(files[1], { ...legacy, voice: null });
  assert.deepEqual(files[2]?.voice, { ms: 1000, levels: '', transcript: '', reply: { to: 'quote', thread: false } });
  await messages.forwardFile('sp_fixture', files[0]!);
  await messages.forwardFile('sp_fixture', files[1]!);
  assert.deepEqual(queued[0], { ...file, voice: { ...file.voice, reply: null } });
  assert.deepEqual(queued[1], files[1]);
  assert.deepEqual(files[0]?.voice?.reply, { to: 'source-entry', thread: true });
});

test('web recordings post a FileDoc with native voice metadata after uploading', async (t) => {
  install(t);
  const file = await attachment();
  const head = new TextEncoder().encode(JSON.stringify({ doc: file, uploads: [{ blob: file.chunks[0]!.blob, len: 1 }] }));
  const framed = new Uint8Array(head.length + 5);
  new DataView(framed.buffer).setUint32(0, head.length, true); framed.set(head, 4);
  const voice = { ms: 2300, levels: '04ff80', transcript: '', reply: null };
  const codec = messagingCore();
  let queued: FileDoc | undefined;
  const core = { ...codec, bytes: async (op: CoreOperation) => op.kind === 'seal_file' ? framed : codec.bytes(op),
    text: async (op: CoreOperation) => {
      if (op.kind !== 'queue_file') return codec.text(op);
      queued = JSON.parse(op.doc);
      return JSON.stringify({ snapshot: 'sent voice', frames: [], conversations: [conversation] });
    } };
  const messages = new Messages({ core, session: linked, vault: { ...vault, save: async () => {} }, changed: () => {} });
  t.after(() => messages.stop());
  t.mock.method(globalThis, 'fetch', async () => new Response(null, { status: 200 }));
  await messages.start(); await authenticate();
  await messages.sendFile('sp_fixture', { name: file.name, mime: file.mime, bytes: new Uint8Array([1, 2, 3]), voice });
  assert.deepEqual(queued, { ...file, voice });
});

test('malformed attachments stay local while received updates persist, synchronize and reload', async (t) => {
  install(t);
  const file = await attachment();
  const chunk = file.chunks[0]!;
  const entry = { id: 'text', entry: 'ordered', author: login.identity, name: 'Member', text: 'Still readable',
    at_ms: 1, delivery: 'confirmed', failure: null, file: null };
  const invalid = [
    { ...file, chunks: [{ ...chunk, blob: 'bad' }] },
    { ...file, chunks: [{ ...chunk, sha256: 'bad' }] },
    { ...file, chunks: [{ ...chunk, key: 'bad' }] },
    { ...file, thumb: { ...chunk, blob: 'bad' } },
  ];
  const received = [{ ...conversation, messages: [
    ...invalid.map((file, i) => ({ ...entry, id: `invalid-${i}`, text: '', file })),
    { ...entry, id: 'valid', text: '', file }, entry,
  ] }, { ...conversation, id: 'sp_other', messages: [entry] }];
  let saved: StoredSession = linked;
  const states: MessagesState[] = [];
  const core = messagingCore();
  const text = core.text;
  core.text = async (operation: CoreOperation) => {
    if (operation.kind === 'restore' && operation.snapshot === 'received attachments') return JSON.stringify(received);
    if (operation.kind === 'bootstrap' && saved.kind === 'linked' && saved.snapshot === 'received attachments') {
      return JSON.stringify({ snapshot: saved.snapshot, frames: [], conversations: received });
    }
    if (operation.kind === 'receive') return JSON.stringify({ snapshot: 'received attachments', frames: [[9]], conversations: received });
    return text(operation);
  };
  const open = (session: typeof linked) => {
    const messages = new Messages({ core, session, vault: { ...vault, save: async (session) => { saved = session; } },
      changed: (state) => states.push(state) });
    t.after(() => messages.stop());
    return messages;
  };
  const check = () => {
    const state = states.at(-1);
    assert.equal(state?.kind, 'active');
    if (state?.kind !== 'active') assert.fail('message controller failed');
    const entries = state.conversations[0]!.messages;
    for (const [i] of invalid.entries()) {
      assert.equal(entries[i]!.id, `invalid-${i}`);
      assert.equal(entries[i]!.file, null);
      assert.equal(entries[i]!.text, 'Anexo indisponível');
    }
    assert.deepEqual(entries[invalid.length]!.file, file);
    assert.equal(entries.at(-1)!.text, entry.text);
    assert.equal(state.conversations[1]!.messages[0]!.text, entry.text);
    assert.equal(core.closed, false);
  };
  const messages = open(linked);
  await messages.start(); await authenticate();
  socket().frame({ kind: 'event' }); await settle();
  check();
  assert.equal(saved.kind === 'linked' && saved.snapshot, 'received attachments');
  assert.deepEqual(socket().sent.at(-1), new Uint8Array([9]));
  await messages.send({ space: 'sp_other', text: 'Can still send' });
  assert.deepEqual(socket().sent.at(-1), new Uint8Array([8]));
  // Save another received update, then restore that same unmodified snapshot.
  socket().frame({ kind: 'event' }); await settle(); check();
  messages.stop();
  assert.equal(saved.kind, 'linked');
  if (saved.kind !== 'linked') assert.fail('received snapshot was not persisted');
  const reloaded = open(saved);
  await reloaded.start(); check();
  await authenticate(); check();
  const state = states.at(-1);
  assert.equal(state?.kind === 'active' && state.connection.kind, 'ready');
  assert.equal(states.some((state) => state.kind === 'failed'), false);
});

function mediaMessages(t: test.TestContext, bytes: (op: CoreOperation) => Promise<Uint8Array>) {
  const messages = new Messages({ core: { ...messagingCore(), bytes }, session: linked,
    vault: { ...vault, save: async () => {} }, changed: () => {} });
  t.after(() => messages.stop());
  return messages;
}

test('files and thumbnails require the complete digest and exact decrypted lengths', async (t) => {
  const data = new Uint8Array([1, 2, 3]);
  const file = await attachment(data);
  let opened = data;
  const messages = mediaMessages(t, async () => opened);
  t.mock.method(globalThis, 'fetch', async () => new Response(new Uint8Array(file.bytes + 40)));
  assert.deepEqual(new Uint8Array(await (await messages.openFile(file)).arrayBuffer()), data);
  await assert.rejects(messages.openFile({ ...file, sha256: '00'.repeat(32) }));
  for (const length of [2, 4]) { opened = new Uint8Array(length); await assert.rejects(messages.openFile(file)); }
  opened = data;
  const thumb = { ...file, thumb: file.chunks[0]! };
  assert.equal((await messages.openFile(thumb, true)).type, 'image/png');
  await assert.rejects(messages.openFile({ ...thumb, thumb: { ...thumb.thumb, sha256: '00'.repeat(32) } }, true));
});

test('native media bounds reject hostile descriptors before any network or worker work', async (t) => {
  const file = await attachment();
  const messages = mediaMessages(t, async () => { assert.fail('worker called for invalid descriptor'); });
  const fetching = t.mock.method(globalThis, 'fetch', async () => { assert.fail('fetch called for invalid descriptor'); });
  const chunk = file.chunks[0]!;
  const invalid = [
    { ...file, bytes: 0 }, { ...file, bytes: (512 << 20) + 1 }, { ...file, sha256: 'bad' },
    { ...file, chunks: [] }, { ...file, chunks: Array(514).fill(chunk), bytes: 3 * 514 },
    { ...file, chunks: [{ ...chunk, bytes: 0 }] },
    { ...file, chunks: [{ ...chunk, bytes: (16 << 20) + 1 }], bytes: (16 << 20) + 1 },
    { ...file, chunks: Array(100).fill({ ...chunk, bytes: 16 << 20 }), bytes: 100 * (16 << 20) },
    { ...file, bytes: file.bytes + 1 }, { ...file, name: 'é'.repeat(128 << 10) },
    { ...file, thumb: { ...chunk, bytes: (2 << 20) + 1 } },
  ];
  for (const doc of invalid) {
    await assert.rejects(messages.openFile(doc));
    await assert.rejects(messages.openFile(doc, true));
  }
  assert.equal(fetching.mock.callCount(), 0);
});

test('encrypted chunk reads reject truncation and cancel streamed overruns before decryption', async (t) => {
  const file = await attachment();
  const messages = mediaMessages(t, async () => { assert.fail('invalid ciphertext reached worker'); });
  t.mock.method(globalThis, 'fetch', async () => new Response(new Uint8Array(file.bytes + 39)));
  await assert.rejects(messages.openFile(file));
  let cancelled = false;
  t.mock.method(globalThis, 'fetch', async () => new Response(new ReadableStream({
    start(controller) { controller.enqueue(new Uint8Array(file.bytes + 40)); controller.enqueue(new Uint8Array(1)); },
    cancel() { cancelled = true; },
  })));
  await assert.rejects(messages.openFile(file));
  assert.equal(cancelled, true);
});

test('valid chunks assemble in manifest order and snapshot metadata before fetching', async (t) => {
  const first = await attachment(new Uint8Array([1, 2]));
  const last = await attachment(new Uint8Array([3]));
  const file = await attachment(); file.chunks = [...first.chunks, ...last.chunks];
  const messages = mediaMessages(t, async (op) => {
    if (op.kind !== 'open_chunk') throw new Error('Unexpected test operation.');
    return new Uint8Array(JSON.parse(op.chunk).bytes === 2 ? [1, 2] : [3]);
  });
  let calls = 0;
  t.mock.method(globalThis, 'fetch', async () => {
    file.bytes = 0; file.sha256 = '00'.repeat(32); file.chunks.length = 0;
    return new Response(new Uint8Array((calls++ ? 1 : 2) + 40));
  });
  assert.deepEqual(new Uint8Array(await (await messages.openFile(file)).arrayBuffer()), new Uint8Array([1, 2, 3]));
  assert.equal(calls, 2);
});

test('upload retries stop after four attempts for HTTP failures and network errors', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const file = await attachment();
  const head = new TextEncoder().encode(JSON.stringify({ doc: file, uploads: [{ blob: file.chunks[0]!.blob, len: 1 }] }));
  const framed = new Uint8Array(head.length + 5);
  new DataView(framed.buffer).setUint32(0, head.length, true); framed.set(head, 4);
  const messages = mediaMessages(t, async (op) => op.kind === 'seal_file' ? framed : new Uint8Array());
  for (const status of [429, 500, 503, 0]) {
    const fetching = t.mock.method(globalThis, 'fetch', async () => {
      if (!status) throw new Error('offline');
      return new Response(null, { status });
    });
    const sending = messages.sendFile('sp_fixture', { name: file.name, mime: file.mime, bytes: new Uint8Array([1]) });
    const rejected = assert.rejects(sending, /Sem conexão/);
    await settle();
    for (const delay of [800, 1600, 3200]) { t.mock.timers.tick(delay); await settle(); }
    await rejected;
    assert.equal(fetching.mock.callCount(), 4);
    fetching.mock.restore();
  }
  for (const status of [403, 413]) {
    const fetching = t.mock.method(globalThis, 'fetch', async () => new Response(null, { status }));
    await assert.rejects(messages.sendFile('sp_fixture', { name: file.name, mime: file.mime, bytes: new Uint8Array([1]) }),
      status === 413 ? /grande demais/ : /recusou/);
    assert.equal(fetching.mock.callCount(), 1); fetching.mock.restore();
  }
  let responses = 0;
  const fetching = t.mock.method(globalThis, 'fetch', async () => new Response(null, { status: ++responses < 2 ? 503 : 200 }));
  let done = 0;
  // Upload succeeds after the retry; this unstarted controller then refuses the post.
  const sending = assert.rejects(messages.sendFile('sp_fixture', { name: file.name, mime: file.mime, bytes: new Uint8Array([1]) }, () => { done++; }), /Sem conexão agora/);
  await settle(); t.mock.timers.tick(800); await settle(); await sending;
  assert.equal(fetching.mock.callCount(), 2); assert.equal(done, 1);
});

test('upload failure keeps the queued voice note as failed instead of losing it', async (t) => {
  install(t);
  const file = await attachment();
  const head = new TextEncoder().encode(JSON.stringify({ doc: { ...file, voice: { ms: 1200, levels: '04', transcript: '', reply: null } }, uploads: [{ blob: file.chunks[0]!.blob, len: 1 }] }));
  const framed = new Uint8Array(head.length + 5);
  new DataView(framed.buffer).setUint32(0, head.length, true); framed.set(head, 4);
  const voice = { ms: 1200, levels: '04', transcript: '', reply: null };
  const codec = messagingCore();
  const failed: string[] = [];
  const core = { ...codec,
    bytes: async (op: CoreOperation) => op.kind === 'seal_file' ? framed : codec.bytes(op),
    text: async (op: CoreOperation) => {
      if (op.kind === 'queue_file') {
        const doc = JSON.parse(op.doc);
        return JSON.stringify({ snapshot: 'queued', frames: [], conversations: [{ ...conversation, messages: [{
          id: 'pending-voice', entry: null, author: login.identity, name: 'Me', text: '', at_ms: 1,
          delivery: 'sending', failure: null, ticks: null, edited: false, deleted: false, forwarded: false,
          reactions: [], quote: null, thread: null, replies: 0, repliers: [], last_reply_ms: null, file: doc,
        }] }] });
      }
      if (op.kind === 'fail_outbox') {
        failed.push(op.client_id);
        return JSON.stringify({ snapshot: 'failed', frames: [], conversations: [{ ...conversation, messages: [{
          id: op.client_id, entry: null, author: login.identity, name: 'Me', text: '', at_ms: 1,
          delivery: 'failed', failure: op.reason, ticks: null, edited: false, deleted: false, forwarded: false,
          reactions: [], quote: null, thread: null, replies: 0, repliers: [], last_reply_ms: null,
          file: { ...file, voice },
        }] }] });
      }
      return codec.text(op);
    },
  };
  const states: MessagesState[] = [];
  const messages = new Messages({ core, session: linked, vault: { ...vault, save: async () => {} }, changed: (state) => states.push(state) });
  t.after(() => messages.stop());
  t.mock.method(globalThis, 'fetch', async () => new Response(null, { status: 503 }));
  await messages.start(); await authenticate();
  await assert.rejects(() => messages.sendFile('sp_fixture', { name: file.name, mime: file.mime, bytes: new Uint8Array([1]), voice }));
  assert.deepEqual(failed, ['pending-voice']);
  const state = states.at(-1);
  if (state?.kind !== 'active') assert.fail('expected active');
  const msg = state.conversations[0]!.messages[0]!;
  assert.equal(msg.delivery, 'failed');
  assert.deepEqual(msg.file?.voice, voice);
});
