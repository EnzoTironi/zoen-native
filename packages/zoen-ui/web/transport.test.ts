import test from 'node:test';
import assert from 'node:assert/strict';
import { AuthenticatedTransport, type ConnectionState } from './transport.ts';
import type { CoreOperation } from './worker-client.ts';
import { Messages, type MessagesState } from './messages.ts';
import { parseSession, type LinkedAccount, type PendingLink, type StoredSession } from './session.ts';
import { Pairing, type PairingState } from './pairing.ts';

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

const deviceKey = 'ab'.repeat(32);
const account: LinkedAccount = {
  identity: 'cd'.repeat(32), device: deviceKey, cert: 'cert', name: 'Ana', handle: 'ana',
  relay: 'https://relay.example', primary: 'ef'.repeat(32), primary_hpke: 'hpke', hpke_secret: 'mine',
  link_secret: 'link', agreement_secret: 'agree', profile_key: 'profile', profile_key_version: 1,
  profile_seen_version: 1, profile_fields: '{}',
};
const pendingLink: PendingLink = {
  device: deviceKey, relay: 'https://relay.example', hpke_secret: 'mine', secret: 'link',
  code: 'zoen-link:1:' + '00'.repeat(96), check: '123456', box_id: 'box-1',
};

function recordedCore(ops: CoreOperation[]) {
  return {
    bytes: async (operation: CoreOperation) => {
      ops.push(operation);
      return new TextEncoder().encode(operation.kind === 'request' ? JSON.stringify(operation) : operation.kind);
    },
    text: async (operation: CoreOperation) => {
      ops.push(operation);
      if (operation.kind === 'decode_server') return new TextDecoder().decode(operation.bytes);
      if (operation.kind === 'complete_link') return JSON.stringify(account);
      throw new Error(`Unexpected test operation ${operation.kind}.`);
    },
  };
}

async function until(ready: () => boolean) {
  const started = Date.now();
  while (!ready()) {
    if (Date.now() - started > 2000) throw new Error('timed out');
    await settle();
  }
}

test('the browser receipts a link only after the account save, and a reload receipts again', async (t) => {
  install(t);
  const ops: CoreOperation[] = [];
  const saves: StoredSession[] = [];
  let releaseSave = () => {};
  const held = new Promise<void>((resolve) => { releaseSave = resolve; });
  const vault = {
    device: async () => deviceKey,
    sign: async () => new Uint8Array(64),
    load: async () => saves.at(-1) ?? { kind: 'pending' as const, pending: pendingLink },
    save: async (session: StoredSession) => {
      saves.push(JSON.parse(JSON.stringify(session)) as StoredSession);
      if (session.kind === 'pending' && session.confirming) await held;
    },
  };
  const states: PairingState[] = [];
  const pairing = new Pairing(vault, recordedCore(ops), (state) => states.push(state));
  t.after(() => pairing.stop());
  await pairing.start('https://relay.example', { kind: 'pending', pending: pendingLink });
  socket().open(); await settle();
  socket().frame(challenge); await settle();
  socket().frame({ kind: 'ready', identity: deviceKey, registered: false });
  await until(() => ops.some((op) => op.kind === 'request' && op.op === 'fetch_link'));
  const fetch = ops.find((op) => op.kind === 'request' && op.op === 'fetch_link');
  assert.ok(fetch && fetch.kind === 'request');
  socket().frame({ kind: 'response', id: fetch.requestId, value: { kind: 'link', sealed: [9] } });
  await until(() => saves.some((session) => session.kind === 'pending' && session.confirming !== undefined));
  assert.equal(ops.some((op) => op.kind === 'request' && op.op === 'ack_link'), false);
  const confirming = saves.find((session) => session.kind === 'pending' && session.confirming);
  assert.ok(confirming);
  const reloaded = parseSession(JSON.stringify(confirming));
  assert.equal(reloaded.kind, 'pending');
  if (reloaded.kind === 'pending') assert.deepEqual(reloaded.confirming, account);
  releaseSave();
  await until(() => ops.some((op) => op.kind === 'request' && op.op === 'ack_link'));
  const receipt = ops.find((op) => op.kind === 'request' && op.op === 'ack_link');
  assert.ok(receipt && receipt.kind === 'request');
  assert.equal(receipt.value, pendingLink.box_id);
  socket().frame({ kind: 'response', id: receipt.requestId, value: { kind: 'done' } });
  await until(() => states.at(-1)?.kind === 'linked');
  assert.equal(saves.at(-1)?.kind, 'linked');

  const again: CoreOperation[] = [];
  const resumed: PairingState[] = [];
  const second = new Pairing(
    { ...vault, save: async (session: StoredSession) => { saves.push(session); } },
    recordedCore(again),
    (state) => resumed.push(state));
  t.after(() => second.stop());
  await second.start('https://relay.example', reloaded);
  socket().open(); await settle();
  socket().frame(challenge); await settle();
  socket().frame({ kind: 'ready', identity: deviceKey, registered: false });
  await until(() => again.some((op) => op.kind === 'request' && op.op === 'ack_link'));
  assert.equal(again.some((op) => op.kind === 'complete_link' || (op.kind === 'request' && op.op === 'fetch_link')), false);
  const retry = again.find((op) => op.kind === 'request' && op.op === 'ack_link');
  assert.ok(retry && retry.kind === 'request');
  socket().frame({ kind: 'response', id: retry.requestId, value: { kind: 'done' } });
  await until(() => resumed.at(-1)?.kind === 'linked');
  assert.equal(saves.at(-1)?.kind, 'linked');
});

function memoryVault(initial: StoredSession) {
  const saves: StoredSession[] = [];
  let current = initial;
  const vault = {
    device: async () => deviceKey,
    sign: async () => new Uint8Array(64),
    load: async () => current,
    save: async (session: StoredSession) => {
      current = JSON.parse(JSON.stringify(session)) as StoredSession;
      saves.push(current);
    },
  };
  return { saves, vault };
}

function barePending(session: StoredSession) {
  return session.kind === 'pending' && session.confirming === undefined;
}

test('start resumes a stored confirming account and never writes pending', async (t) => {
  install(t);
  const ops: CoreOperation[] = [];
  const { saves, vault } = memoryVault({ kind: 'pending', pending: pendingLink, confirming: account });
  const states: PairingState[] = [];
  const pairing = new Pairing(vault, recordedCore(ops), (state) => states.push(state));
  t.after(() => pairing.stop());
  await pairing.start('https://relay.example', { kind: 'pending', pending: pendingLink });
  assert.equal(saves.some(barePending), false);
  assert.equal(states.some((state) => state.kind === 'confirming'), true);
  assert.equal(states.some((state) => state.kind === 'waiting' || state.kind === 'failed' || state.kind === 'starting'), false);
  socket().open(); await settle();
  socket().frame(challenge); await settle();
  socket().frame({ kind: 'ready', identity: deviceKey, registered: false });
  await until(() => ops.some((op) => op.kind === 'request' && op.op === 'ack_link'));
  assert.equal(ops.some((op) => op.kind === 'request' && op.op === 'fetch_link' || op.kind === 'complete_link'), false);
  const receipt = ops.find((op) => op.kind === 'request' && op.op === 'ack_link');
  assert.ok(receipt && receipt.kind === 'request');
  socket().frame({ kind: 'response', id: receipt.requestId, value: { kind: 'done' } });
  await until(() => states.at(-1)?.kind === 'linked');
  assert.equal(saves.some(barePending), false);
  assert.equal(saves.at(-1)?.kind, 'linked');
});

type MiniNode = {
  nodeType: number; nodeName: string; id?: string; alt?: string; nodeValue: string | null;
  childNodes: MiniNode[]; textContent: string; focus: () => void; contains: (other: MiniNode) => boolean;
};
type GateModule = { PairingGate: (props: { runtime: { vault: ReturnType<typeof memoryVault>['vault']; core: ReturnType<typeof recordedCore> }; session: StoredSession; linked: (session: StoredSession) => void }) => unknown };
type BrowserHost = {
  document: { body: MiniNode; activeElement: MiniNode | null; createElement: (tag: string) => MiniNode };
  act: (work: () => Promise<void>) => Promise<void>;
  createRoot: (container: MiniNode) => { render: (node: unknown) => void; unmount: () => void };
  createElement: (type: unknown, props: unknown) => unknown;
  PairingGate: GateModule['PairingGate'];
};

function relink(parent: { childNodes: Array<{ previousSibling: unknown; nextSibling: unknown }> }) {
  for (let i = 0; i < parent.childNodes.length; i++) {
    const child = parent.childNodes[i];
    if (!child) continue;
    child.previousSibling = parent.childNodes[i - 1] ?? null;
    child.nextSibling = parent.childNodes[i + 1] ?? null;
  }
}

let hostPromise: Promise<BrowserHost> | undefined;
function browserHost(): Promise<BrowserHost> {
  hostPromise ??= (async () => {
    const owner = {} as { activeElement: MiniNode | null; documentElement: MiniNode; body: MiniNode; head: MiniNode };
    function createNode(type: number, tag = ''): MiniNode {
      const node = {
        nodeType: type,
        nodeName: type === 3 ? '#text' : String(tag).toUpperCase(),
        tagName: type === 1 ? String(tag).toUpperCase() : undefined,
        namespaceURI: 'http://www.w3.org/1999/xhtml',
        ownerDocument: owner,
        parentNode: null as MiniNode | null,
        childNodes: [] as MiniNode[],
        style: {},
        attributes: new Map<string, string>(),
        nodeValue: type === 3 ? '' : null,
        className: '',
        previousSibling: null as MiniNode | null,
        nextSibling: null as MiniNode | null,
        innerHTML: '',
        dataset: {},
        id: '',
        appendChild(child: MiniNode) {
          if (child.parentNode) child.parentNode.removeChild(child);
          child.parentNode = this;
          this.childNodes.push(child);
          relink(this);
          return child;
        },
        removeChild(child: MiniNode) {
          const index = this.childNodes.indexOf(child);
          if (index < 0) throw new Error('not a child');
          this.childNodes.splice(index, 1);
          child.parentNode = null;
          relink(this);
          return child;
        },
        insertBefore(child: MiniNode, before: MiniNode | null) {
          if (child.parentNode) child.parentNode.removeChild(child);
          child.parentNode = this;
          const index = before ? this.childNodes.indexOf(before) : this.childNodes.length;
          this.childNodes.splice(index < 0 ? this.childNodes.length : index, 0, child);
          relink(this);
          return child;
        },
        setAttribute(name: string, value: string) { this.attributes.set(name, String(value)); },
        getAttribute(name: string) { return this.attributes.has(name) ? this.attributes.get(name) ?? null : null; },
        removeAttribute(name: string) { this.attributes.delete(name); },
        hasAttribute(name: string) { return this.attributes.has(name); },
        addEventListener() {},
        removeEventListener() {},
        dispatchEvent() { return true; },
        focus() { owner.activeElement = this; },
        blur() {},
        contains(other: MiniNode): boolean {
          if (other === this) return true;
          return this.childNodes.some((child) => child.contains(other));
        },
        get firstChild() { return this.childNodes[0] ?? null; },
        get lastChild() { return this.childNodes.at(-1) ?? null; },
        get textContent() {
          if (this.nodeType === 3) return this.nodeValue ?? '';
          return this.childNodes.map((child) => child.textContent).join('');
        },
        set textContent(value: string) {
          this.childNodes.length = 0;
          if (value) {
            const text = createNode(3);
            text.nodeValue = String(value);
            this.appendChild(text);
          }
        },
      };
      return node;
    }
    const document = owner as typeof owner & {
      nodeType: number; nodeName: string; createElement: (tag: string) => MiniNode;
      createElementNS: (ns: string, tag: string) => MiniNode; createTextNode: (text: string) => MiniNode;
      getElementById: (id: string) => MiniNode | null; createDocumentFragment: () => MiniNode;
      addEventListener: () => void; removeEventListener: () => void; querySelector: () => null; querySelectorAll: () => [];
      defaultView: unknown;
    };
    document.nodeType = 9;
    document.nodeName = '#document';
    document.activeElement = null;
    document.createElement = (tag: string) => createNode(1, tag);
    document.createElementNS = (_ns: string, tag: string) => createNode(1, tag);
    document.createTextNode = (text: string) => { const node = createNode(3); node.nodeValue = String(text); return node; };
    document.createDocumentFragment = () => createNode(11);
    document.addEventListener = () => {};
    document.removeEventListener = () => {};
    document.querySelector = () => null;
    document.querySelectorAll = () => [];
    document.getElementById = (id: string) => {
      const walk = (node: MiniNode | null): MiniNode | null => {
        if (!node) return null;
        if (node.id === id) return node;
        for (const child of node.childNodes) { const found = walk(child); if (found) return found; }
        return null;
      };
      return walk(document.documentElement);
    };
    document.documentElement = document.createElement('html');
    document.head = document.createElement('head');
    document.body = document.createElement('body');
    document.documentElement.appendChild(document.head);
    document.documentElement.appendChild(document.body);
    const windowHost = {
      document, navigator: { userAgent: 'node' },
      HTMLElement: class HTMLElement {}, HTMLIFrameElement: class HTMLIFrameElement {},
      Element: class Element {}, Node: class Node {},
      addEventListener() {}, removeEventListener() {},
      getComputedStyle: () => ({ getPropertyValue: () => '' }),
      requestAnimationFrame: (fn: () => void) => setTimeout(fn, 0),
      cancelAnimationFrame: (id: ReturnType<typeof setTimeout>) => clearTimeout(id),
    };
    document.defaultView = windowHost;
    const root = document.createElement('div');
    root.id = 'root';
    document.body.appendChild(root);
    Object.assign(globalThis, {
      window: windowHost, document, HTMLElement: windowHost.HTMLElement, HTMLIFrameElement: windowHost.HTMLIFrameElement,
      Element: windowHost.Element, Node: windowHost.Node, getComputedStyle: windowHost.getComputedStyle,
      IS_REACT_ACT_ENVIRONMENT: true, location: { pathname: '/', search: '' },
    });
    const react = await import('react');
    const dom = await import('react-dom/client');
    const act = react.act;
    const esbuild = await import('esbuild');
    const { mkdir } = await import('node:fs/promises');
    const { unlinkSync } = await import('node:fs');
    const { fileURLToPath, pathToFileURL } = await import('node:url');
    const cacheUrl = new URL('../node_modules/.cache/u4-pairing-gate.mjs', import.meta.url);
    const cache = fileURLToPath(cacheUrl);
    await mkdir(fileURLToPath(new URL('.', cacheUrl)), { recursive: true });
    const build = esbuild.build ?? esbuild.default?.build;
    await build({
      entryPoints: [fileURLToPath(new URL('./main.tsx', import.meta.url))],
      bundle: true, format: 'esm', platform: 'node', outfile: cache, jsx: 'automatic', logLevel: 'silent',
      external: ['react', 'react/jsx-runtime', 'react/jsx-dev-runtime', 'qrcode'],
      plugins: [
        { name: 'css', setup(plugin: { onResolve: Function; onLoad: Function }) {
          plugin.onResolve({ filter: /\.css$/ }, (args: { path: string }) => ({ path: args.path, namespace: 'css' }));
          plugin.onLoad({ filter: /.*/, namespace: 'css' }, () => ({ contents: 'export default {}', loader: 'js' }));
        } },
        { name: 'client', setup(plugin: { onResolve: Function; onLoad: Function }) {
          plugin.onResolve({ filter: /^react-dom\/client$/ }, () => ({ path: 'react-dom-client', namespace: 'stub' }));
          plugin.onLoad({ filter: /.*/, namespace: 'stub' }, () => ({ contents: 'export function createRoot(){ return { render() {}, unmount() {} }; }', loader: 'js' }));
        } },
      ],
    });
    const gate = await import(pathToFileURL(cache).href) as GateModule;
    process.once('exit', () => { try { unlinkSync(cache); } catch { /* The bundle is only a test artifact. */ } });
    return { document, act, createRoot: dom.createRoot, createElement: react.createElement, PairingGate: gate.PairingGate };
  })();
  return hostPromise;
}

async function flush(act: BrowserHost['act'], tick?: (ms: number) => void) {
  await act(async () => {
    tick?.(0);
    for (let i = 0; i < 12; i++) await new Promise<void>((resolve) => setImmediate(resolve));
  });
}

function treeText(node: MiniNode): string {
  const parts: string[] = [];
  const walk = (current: MiniNode & { alt?: string }) => {
    if (current.nodeType === 3) parts.push(current.nodeValue ?? '');
    if (typeof current.alt === 'string') parts.push(current.alt);
    for (const child of current.childNodes) walk(child);
  };
  walk(node);
  return parts.join(' ');
}

function buttonLabels(node: MiniNode): string[] {
  const labels: string[] = [];
  const walk = (current: MiniNode) => {
    if (current.nodeName === 'BUTTON') labels.push(current.textContent);
    for (const child of current.childNodes) walk(child);
  };
  walk(node);
  return labels;
}

function hasTag(node: MiniNode, name: string): boolean {
  if (node.nodeName === name) return true;
  return node.childNodes.some((child) => hasTag(child, name));
}

function mountGate(host: BrowserHost, session: StoredSession, vault: ReturnType<typeof memoryVault>['vault'], ops: CoreOperation[], linked: (session: StoredSession) => void) {
  const container = host.document.createElement('div');
  host.document.body.appendChild(container);
  const root = host.createRoot(container);
  root.render(host.createElement(host.PairingGate, {
    runtime: { vault, core: recordedCore(ops) }, session, linked,
  }));
  return { container, root };
}

test('a confirming reload shows the status line and finishes linked', async (t) => {
  const host = await browserHost();
  install(t);
  const ops: CoreOperation[] = [];
  const stored: StoredSession = { kind: 'pending', pending: pendingLink, confirming: account };
  const { saves, vault } = memoryVault(stored);
  const linked: StoredSession[] = [];
  const mounted = mountGate(host, stored, vault, ops, (session) => linked.push(session));
  t.after(() => mounted.root.unmount());
  await flush(host.act, (ms) => t.mock.timers.tick(ms));
  const shown = () => treeText(mounted.container);
  const linkForm = () => buttonLabels(mounted.container).includes('Link this browser');
  assert.match(shown(), /Confirmando a ligação com o seu telefone/);
  assert.equal(linkForm(), false);
  assert.equal(hasTag(mounted.container, 'IMG'), false);
  assert.equal(shown().includes(pendingLink.check), false);
  assert.equal(shown().includes('Relay address'), false);
  assert.match(host.document.activeElement?.textContent ?? '', /Confirmando a ligação com o seu telefone/);
  await until(() => Socket.instances.length > 0);
  socket().open(); await flush(host.act, (ms) => t.mock.timers.tick(ms));
  socket().frame(challenge); await flush(host.act, (ms) => t.mock.timers.tick(ms));
  socket().frame({ kind: 'ready', identity: deviceKey, registered: false });
  await until(() => ops.some((op) => op.kind === 'request' && op.op === 'ack_link'));
  assert.equal(ops.some((op) => op.kind === 'request' && op.op === 'fetch_link'), false);
  assert.equal(saves.some(barePending), false);
  const receipt = ops.find((op) => op.kind === 'request' && op.op === 'ack_link');
  assert.ok(receipt && receipt.kind === 'request');
  socket().frame({ kind: 'response', id: receipt.requestId, value: { kind: 'done' } });
  await until(() => linked.length === 1);
  await flush(host.act, (ms) => t.mock.timers.tick(ms));
  assert.equal(linked[0]?.kind, 'linked');
  assert.equal(saves.at(-1)?.kind, 'linked');
  assert.equal(linkForm(), false);
});

test('a timed-out ack keeps the account, hides the link form, and retries into linked', async (t) => {
  const host = await browserHost();
  install(t);
  const ops: CoreOperation[] = [];
  const stored: StoredSession = { kind: 'pending', pending: pendingLink };
  const { saves, vault } = memoryVault(stored);
  const linked: StoredSession[] = [];
  const mounted = mountGate(host, stored, vault, ops, (session) => linked.push(session));
  t.after(() => mounted.root.unmount());
  await flush(host.act, (ms) => t.mock.timers.tick(ms));
  await until(() => Socket.instances.length > 0);
  socket().open(); await flush(host.act, (ms) => t.mock.timers.tick(ms));
  socket().frame(challenge); await flush(host.act, (ms) => t.mock.timers.tick(ms));
  socket().frame({ kind: 'ready', identity: deviceKey, registered: false });
  await until(() => ops.some((op) => op.kind === 'request' && op.op === 'fetch_link'));
  const fetch = ops.find((op) => op.kind === 'request' && op.op === 'fetch_link');
  assert.ok(fetch && fetch.kind === 'request');
  socket().frame({ kind: 'response', id: fetch.requestId, value: { kind: 'link', sealed: [9] } });
  await until(() => saves.some((session) => session.kind === 'pending' && session.confirming !== undefined));
  await flush(host.act, (ms) => t.mock.timers.tick(ms));
  const shown = () => treeText(mounted.container);
  const linkForm = () => buttonLabels(mounted.container).includes('Link this browser');
  assert.match(shown(), /Confirmando a ligação com o seu telefone/);
  assert.equal(linkForm(), false);
  assert.equal(ops.some((op) => op.kind === 'request' && op.op === 'ack_link'), true);
  await host.act(async () => { t.mock.timers.tick(15_000); });
  await flush(host.act, (ms) => t.mock.timers.tick(ms));
  const confirming = saves.filter((session) => session.kind === 'pending' && session.confirming);
  assert.equal(confirming.length > 0, true);
  assert.equal(saves.slice(saves.findIndex((session) => session.kind === 'pending' && session.confirming)).some(barePending), false);
  assert.deepEqual(confirming.at(-1)?.kind === 'pending' ? confirming.at(-1)?.confirming : undefined, account);
  assert.equal(linkForm(), false);
  assert.match(shown(), /Tentar de novo/);
  assert.match(shown(), /Confirmando a ligação com o seu telefone/);
  const before = ops.filter((op) => op.kind === 'request' && op.op === 'ack_link').length;
  await host.act(async () => { t.mock.timers.tick(700); });
  await flush(host.act, (ms) => t.mock.timers.tick(ms));
  await until(() => ops.filter((op) => op.kind === 'request' && op.op === 'ack_link').length > before);
  const retry = ops.filter((op) => op.kind === 'request' && op.op === 'ack_link').at(-1);
  assert.ok(retry && retry.kind === 'request');
  socket().frame({ kind: 'response', id: retry.requestId, value: { kind: 'done' } });
  await until(() => linked.length === 1);
  await flush(host.act);
  assert.equal(linked[0]?.kind, 'linked');
  assert.equal(saves.at(-1)?.kind, 'linked');
  assert.equal(linkForm(), false);
  assert.equal(saves.some((session, index) => index > saves.findIndex((item) => item.kind === 'pending' && item.confirming) && barePending(session)), false);
});

test('a lost fetch before confirming keeps the pending box and retries into linked', async (t) => {
  const host = await browserHost();
  install(t);
  const ops: CoreOperation[] = [];
  const stored: StoredSession = { kind: 'pending', pending: pendingLink };
  const { saves, vault } = memoryVault(stored);
  const linked: StoredSession[] = [];
  const mounted = mountGate(host, stored, vault, ops, (session) => linked.push(session));
  t.after(() => mounted.root.unmount());
  await flush(host.act, (ms) => t.mock.timers.tick(ms));
  await until(() => Socket.instances.length > 0);
  socket().open(); await flush(host.act, (ms) => t.mock.timers.tick(ms));
  socket().frame(challenge); await flush(host.act, (ms) => t.mock.timers.tick(ms));
  socket().frame({ kind: 'ready', identity: deviceKey, registered: false });
  await until(() => ops.some((op) => op.kind === 'request' && op.op === 'fetch_link'));
  const linkForm = () => buttonLabels(mounted.container).includes('Link this browser');
  const shown = () => treeText(mounted.container);
  assert.equal(linkForm(), false);
  assert.equal(shown().includes(pendingLink.check), true);
  await host.act(async () => { t.mock.timers.tick(15_000); });
  await flush(host.act, (ms) => t.mock.timers.tick(ms));
  assert.equal(linkForm(), false);
  assert.equal(shown().includes(pendingLink.check), true);
  assert.equal(ops.some((op) => op.kind === 'create_link'), false);
  assert.equal(saves.length > 0, true);
  assert.equal(saves.every((session) => session.kind === 'pending' && session.pending.box_id === pendingLink.box_id && session.confirming === undefined), true);
  const before = ops.filter((op) => op.kind === 'request' && op.op === 'fetch_link').length;
  await host.act(async () => { t.mock.timers.tick(700); });
  await flush(host.act, (ms) => t.mock.timers.tick(ms));
  await until(() => ops.filter((op) => op.kind === 'request' && op.op === 'fetch_link').length > before);
  const again = ops.filter((op) => op.kind === 'request' && op.op === 'fetch_link').at(-1);
  assert.ok(again && again.kind === 'request');
  assert.equal(again.value, pendingLink.box_id);
  assert.equal(linkForm(), false);
  socket().frame({ kind: 'response', id: again.requestId, value: { kind: 'link', sealed: [9] } });
  await until(() => saves.some((session) => session.kind === 'pending' && session.confirming !== undefined));
  await flush(host.act, (ms) => t.mock.timers.tick(ms));
  const receipt = ops.find((op) => op.kind === 'request' && op.op === 'ack_link');
  assert.ok(receipt && receipt.kind === 'request');
  socket().frame({ kind: 'response', id: receipt.requestId, value: { kind: 'done' } });
  await until(() => linked.length === 1);
  await flush(host.act);
  assert.equal(linked[0]?.kind, 'linked');
  assert.equal(linkForm(), false);
  assert.equal(ops.some((op) => op.kind === 'create_link'), false);
});

test('a fetch still inside complete_link cannot replace the linked vault', async (t) => {
  install(t);
  const ops: CoreOperation[] = [];
  const saves: StoredSession[] = [];
  let current: StoredSession = { kind: 'pending', pending: pendingLink };
  let releaseFirst = (_account: string) => {};
  const firstComplete = new Promise<string>((resolve) => { releaseFirst = resolve; });
  let completions = 0;
  const core = {
    bytes: async (operation: CoreOperation) => {
      ops.push(operation);
      return new TextEncoder().encode(operation.kind === 'request' ? JSON.stringify(operation) : operation.kind);
    },
    text: async (operation: CoreOperation) => {
      ops.push(operation);
      if (operation.kind === 'decode_server') return new TextDecoder().decode(operation.bytes);
      if (operation.kind === 'complete_link') {
        completions += 1;
        if (completions === 1) return firstComplete;
        return JSON.stringify(account);
      }
      throw new Error(`Unexpected test operation ${operation.kind}.`);
    },
  };
  const vault = {
    device: async () => deviceKey,
    sign: async () => new Uint8Array(64),
    load: async () => JSON.parse(JSON.stringify(current)) as StoredSession,
    save: async (session: StoredSession) => {
      current = JSON.parse(JSON.stringify(session)) as StoredSession;
      saves.push(current);
    },
  };
  const pairing = new Pairing(vault, core, () => {});
  t.after(() => pairing.stop());
  await pairing.start('https://relay.example', { kind: 'pending', pending: pendingLink });
  socket().open(); await settle();
  socket().frame(challenge); await settle();
  socket().frame({ kind: 'ready', identity: deviceKey, registered: false });
  await until(() => ops.some((op) => op.kind === 'request' && op.op === 'fetch_link'));
  const first = ops.find((op) => op.kind === 'request' && op.op === 'fetch_link');
  assert.ok(first && first.kind === 'request');
  socket().frame({ kind: 'response', id: first.requestId, value: { kind: 'link', sealed: [9] } });
  await until(() => completions === 1);
  const opened = socket();
  opened.close();
  await settle();
  t.mock.timers.tick(1_000);
  await settle();
  assert.ok(socket() !== opened);
  socket().open(); await settle();
  socket().frame(challenge); await settle();
  socket().frame({ kind: 'ready', identity: deviceKey, registered: false });
  for (let i = 0; i < 8; i++) await settle();
  const second = ops.filter((op) => op.kind === 'request' && op.op === 'fetch_link' && op.requestId !== first.requestId).at(-1);
  let secondLinked = false;
  if (second && second.kind === 'request') {
    socket().frame({ kind: 'response', id: second.requestId, value: { kind: 'link', sealed: [9] } });
    await until(() => ops.some((op) => op.kind === 'request' && op.op === 'ack_link'));
    const receipt = ops.find((op) => op.kind === 'request' && op.op === 'ack_link');
    assert.ok(receipt && receipt.kind === 'request');
    socket().frame({ kind: 'response', id: receipt.requestId, value: { kind: 'done' } });
    await until(() => current.kind === 'linked');
    secondLinked = true;
  }
  releaseFirst(JSON.stringify(account));
  for (let i = 0; i < 12; i++) await settle();
  if (!secondLinked) {
    await until(() => ops.some((op) => op.kind === 'request' && op.op === 'ack_link'));
    const receipt = ops.filter((op) => op.kind === 'request' && op.op === 'ack_link').at(-1);
    assert.ok(receipt && receipt.kind === 'request');
    socket().frame({ kind: 'response', id: receipt.requestId, value: { kind: 'done' } });
    await until(() => current.kind === 'linked');
    for (let i = 0; i < 12; i++) await settle();
  }
  assert.equal(current.kind, 'linked');
  assert.equal(saves.at(-1)?.kind, 'linked');
});
