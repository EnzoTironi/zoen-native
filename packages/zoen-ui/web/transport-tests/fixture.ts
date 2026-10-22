// The original single transport/message fixture; no copied socket or state.
import { type CoreOperation } from '../worker-client.ts';
import assert from 'node:assert/strict';
import type test from 'node:test';
import { type StoredSession } from '../session.ts';
import { type FileDoc, Messages } from '../messages.ts';

// These tests exercise connection/storage ordering. Native protobuf/MLS
// interoperability requires the separate owned native relay fixture journey.
export class Socket {
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
  constructor(url: string) {
    this.url = url;
    Socket.instances.push(this);
  }
  open() {
    this.readyState = Socket.OPEN;
    this.onopen?.();
  }
  send(value: Uint8Array) {
    this.sent.push(Uint8Array.from(value));
  }
  frame(value: unknown) {
    this.onmessage?.({ data: new TextEncoder().encode(JSON.stringify(value)).buffer });
  }
  close(code = 1006) {
    this.readyState = 3;
    this.onclose?.({ code });
  }
}
export const login = {
  identity: '11'.repeat(32),
  device: '22'.repeat(32),
  cert: 'cert',
  relay: 'https://relay.example',
};
export const challenge = { kind: 'challenge', nonce: 'nonce', relay: 'relay.example', protocol: 4 };
export const ready = { kind: 'ready', identity: login.identity, registered: true };
export const vault = { sign: async () => new Uint8Array(64) };
export function unitCodec() {
  return {
    bytes: async (operation: CoreOperation) => new TextEncoder().encode(operation.kind),
    text: async (operation: CoreOperation) => {
      if (operation.kind !== 'decode_server') throw new Error('Unexpected test operation.');
      return new TextDecoder().decode(operation.bytes);
    },
  };
}
export function socket() {
  const value = Socket.instances.at(-1);
  assert.ok(value);
  return value;
}
export function install(t: test.TestContext) {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'WebSocket');
  Socket.instances.length = 0;
  Object.defineProperty(globalThis, 'WebSocket', { configurable: true, value: Socket });
  t.mock.timers.enable({ apis: ['setTimeout'] });
  t.after(() => {
    if (descriptor) Object.defineProperty(globalThis, 'WebSocket', descriptor);
    else Reflect.deleteProperty(globalThis, 'WebSocket');
  });
}
export async function settle() {
  await new Promise<void>((resolve) => setImmediate(resolve));
}
export async function authenticate() {
  socket().open();
  await settle();
  socket().frame(challenge);
  await settle();
  socket().frame(ready);
  await settle();
}

export const linked: Extract<StoredSession, { kind: 'linked' }> = {
  kind: 'linked',
  account: {
    ...login,
    name: 'Web fixture',
    handle: 'web_fixture',
    primary: '33'.repeat(32),
    primary_hpke: '',
    hpke_secret: '',
    link_secret: '',
    agreement_secret: '',
    profile_key: '',
    profile_key_version: 0,
    profile_seen_version: 0,
    profile_fields: '',
  },
  snapshot: '',
};
export const conversation = {
  id: 'sp_fixture',
  title: 'Fixture',
  kind: 'group',
  privacy: 'EndToEnd',
  roster: {},
  profiles: [],
  prefs: { pinned: false, archived: false, muted_until_ms: null },
  event_count: '0',
  can_send: true,
  waiting: null,
  unreadable: 0,
  messages: [],
};
export function messagingCore() {
  let closed = false;
  return {
    ...unitCodec(),
    close() {
      closed = true;
    },
    get closed() {
      return closed;
    },
    text: async (operation: CoreOperation) => {
      if (operation.kind === 'cursor_repair_config')
        return JSON.stringify({
          version: 1,
          interval_ms: 60_000,
          timeout_ms: 30_000,
          initial_inactivity_ms: 60_000,
        });
      if (operation.kind === 'document_uploads') return '[]';
      if (operation.kind === 'link_request')
        return JSON.stringify({
          result: { kind: 'idle' },
          update: null,
          downloads: [],
          uploads: [],
        });
      if (operation.kind === 'claim_clock') return '';
      if (operation.kind === 'restore') return JSON.stringify([conversation]);
      if (operation.kind === 'decode_server') return new TextDecoder().decode(operation.bytes);
      if (operation.kind === 'bootstrap')
        return JSON.stringify({
          snapshot: 'durable bootstrap',
          frames: [[7]],
          conversations: [conversation],
        });
      if (operation.kind === 'queue_message')
        return JSON.stringify({
          snapshot: 'new MLS send ratchet',
          frames: [[8]],
          conversations: [conversation],
        });
      if (operation.kind === 'receive')
        return JSON.stringify({ snapshot: 'received', frames: [], conversations: [conversation] });
      throw new Error('Unexpected test core operation.');
    },
  };
}

export const digest = async (data: Uint8Array) =>
  Buffer.from(await crypto.subtle.digest('SHA-256', Uint8Array.from(data))).toString('hex');
export async function attachment(data = new Uint8Array([1, 2, 3])): Promise<FileDoc> {
  const sha256 = await digest(data);
  return {
    name: 'clip.webm',
    path: '',
    mime: 'audio/webm',
    bytes: data.length,
    sha256,
    chunks: [{ sha256, bytes: data.length, blob: '44'.repeat(32), key: '55'.repeat(32) }],
    thumb: null,
    voice: null,
  };
}

export function mediaMessages(
  t: test.TestContext,
  bytes: (op: CoreOperation) => Promise<Uint8Array>,
) {
  const messages = new Messages({
    core: { ...messagingCore(), bytes },
    session: linked,
    vault: { ...vault, save: async () => {} },
    changed: () => {},
  });
  t.after(() => messages.stop());
  return messages;
}

export function stagedAttachmentCore() {
  const codec = messagingCore();
  const frame = new Uint8Array([1, 2, 3]);
  let staged = false,
    uploaded = false;
  const requests: string[] = [];
  const core = {
    ...codec,
    requests,
    get closed() {
      return codec.closed;
    },
    bytes: async (op: CoreOperation) =>
      op.kind === 'seal_file' || op.kind === 'local_chunk' ? frame : codec.bytes(op),
    text: async (op: CoreOperation) => {
      if (op.kind === 'queue_attachment') {
        assert.deepEqual(op.framed, frame);
        requests.push(op.request);
        staged = true;
        return JSON.stringify({
          result: { itemId: 'it_same', messageId: 'msg_same' },
          update: { snapshot: 'retained attachment', frames: [], conversations: [conversation] },
        });
      }
      if (op.kind === 'document_uploads')
        return JSON.stringify(staged && !uploaded ? ['ab'.repeat(32)] : []);
      if (op.kind === 'document_uploaded') {
        uploaded = true;
        return JSON.stringify({
          snapshot: 'uploaded attachment',
          frames: [[61], [62]],
          conversations: [conversation],
        });
      }
      return codec.text(op);
    },
  };
  return core;
}
export const exactAttachment = {
  mutationId: 'attachment_owner_1',
  caption: '  Página 🗂️\nExact.  ',
  replyTo: 'root-hash',
  thread: true,
};
