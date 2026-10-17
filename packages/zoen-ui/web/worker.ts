/// <reference lib="webworker" />
import init, * as core from './core/zoen_web_core.js';
import { bytes, integer, message, object, string } from './boundary.ts';
import type { CoreOperation } from './worker-client.ts';

declare global {
  var zoenSign: (payload: Uint8Array) => Uint8Array;
}
declare const ZOEN_WASM_INTEGRITY: string;

function parseOperation(input: unknown): CoreOperation {
  const value = object(input);
  switch (value.kind) {
    case 'create_link': return { kind: 'create_link', device: string(value.device), relay: string(value.relay) };
    case 'complete_link': return { kind: 'complete_link', pending: string(value.pending), sealed: bytes(value.sealed) };
    case 'hello': return { kind: 'hello', identity: string(value.identity), device: string(value.device), cert: string(value.cert) };
    case 'auth_message': return { kind: 'auth_message', nonce: string(value.nonce), relay: string(value.relay) };
    case 'device_cert_message': return { kind: 'device_cert_message', device: string(value.device) };
    case 'auth': return { kind: 'auth', signature: string(value.signature) };
    case 'request': return { kind: 'request', requestId: integer(value.requestId), op: string(value.op), value: string(value.value) };
    case 'decode_server': return { kind: 'decode_server', bytes: bytes(value.bytes) };
    case 'restore': return { kind: 'restore', account: string(value.account), snapshot: string(value.snapshot) };
    case 'bootstrap': return { kind: 'bootstrap' };
    case 'living_space_home': return { kind: 'living_space_home', space: string(value.space) };
    case 'living_space_mutate': return { kind: 'living_space_mutate', space: string(value.space), command: string(value.command), now: integer(value.now), editId: string(value.editId) };
    case 'page': return { kind: 'page', item: string(value.item) };
    case 'page_download_manifest': return { kind: 'page_download_manifest', item: string(value.item) };
    case 'page_blob_supply': return { kind: 'page_blob_supply', item: string(value.item), blob: string(value.blob), data: bytes(value.data) };
    case 'file_manifest': return { kind: 'file_manifest', item: string(value.item) };
    case 'file_open': return { kind: 'file_open', item: string(value.item), data: bytes(value.data) };
    case 'file_prepare': return { kind: 'file_prepare', space: string(value.space), name: string(value.name), mime: string(value.mime), data: bytes(value.data), now: integer(value.now) };
    case 'file_upload_headers': return { kind: 'file_upload_headers', blob: string(value.blob), now: integer(value.now) };
    case 'file_commit': return { kind: 'file_commit', space: string(value.space), plan: string(value.plan), now: integer(value.now), editId: string(value.editId) };
    case 'page_apply': return { kind: 'page_apply', request: string(value.request), now: integer(value.now) };
    case 'page_create': return { kind: 'page_create', space: string(value.space), title: string(value.title), now: integer(value.now), editId: string(value.editId) };
    case 'receive': return { kind: 'receive', bytes: bytes(value.bytes), now: integer(value.now) };
    case 'queue_message': return { kind: 'queue_message', space: string(value.space), text: string(value.text), now: integer(value.now) };
    case 'queue_post': return { kind: 'queue_post', space: string(value.space), text: string(value.text), replyTo: string(value.replyTo), thread: value.thread === true, forwarded: value.forwarded === true, now: integer(value.now) };
    case 'queue_annotation': return { kind: 'queue_annotation', space: string(value.space), target: string(value.target), note: string(value.note), now: integer(value.now) };
    case 'mark_read': return { kind: 'mark_read', space: string(value.space) };
    case 'queue_file': return { kind: 'queue_file', space: string(value.space), doc: string(value.doc), now: integer(value.now) };
    case 'queue_failed_file': return { kind: 'queue_failed_file', space: string(value.space), doc: string(value.doc), now: integer(value.now), reason: string(value.reason), chunks: string(value.chunks) };
    case 'local_chunk': return { kind: 'local_chunk', blob: string(value.blob) };
    case 'fail_outbox': return { kind: 'fail_outbox', client_id: string(value.client_id), reason: string(value.reason) };
    case 'seal_file': return { kind: 'seal_file', name: string(value.name), mime: string(value.mime), bytes: bytes(value.bytes), thumb: bytes(value.thumb) };
    case 'open_chunk': return { kind: 'open_chunk', chunk: string(value.chunk), sealed: bytes(value.sealed) };
    case 'blob_put_message': return { kind: 'blob_put_message', sha: string(value.sha), ts: integer(value.ts), relay: string(value.relay) };
    case 'queue_join': return { kind: 'queue_join', space: string(value.space), role: string(value.role), code: string(value.code), now: integer(value.now) };
    default: throw new Error('Invalid browser core operation.');
  }
}

let client: core.BrowserCore | undefined;
function restored(): core.BrowserCore {
  if (!client) throw new Error('The encrypted session has not been restored.');
  return client;
}

function run(operation: CoreOperation): string | Uint8Array {
  switch (operation.kind) {
    case 'create_link': return core.create_link(operation.device, operation.relay);
    case 'complete_link': return core.complete_link(operation.pending, operation.sealed);
    case 'hello': return core.hello(operation.identity, operation.device, operation.cert);
    case 'auth_message': return core.auth_message(operation.nonce, operation.relay);
    case 'device_cert_message': return core.device_cert_message(operation.device);
    case 'auth': return core.auth(operation.signature);
    case 'request': return core.request(operation.requestId, operation.op, operation.value);
    case 'decode_server': return core.decode_server(operation.bytes);
    case 'restore': {
      client?.free();
      client = undefined;
      client = new core.BrowserCore(operation.account, operation.snapshot);
      return client.view();
    }
    case 'bootstrap': return restored().bootstrap();
    case 'page': return restored().page(operation.item);
    case 'page_download_manifest': return restored().page_download_manifest(operation.item);
    case 'page_blob_supply': restored().page_blob_supply(operation.item, operation.blob, operation.data); return '';
    case 'file_manifest': return restored().file_manifest(operation.item);
    case 'file_open': return restored().file_open(operation.item, operation.data);
    case 'file_prepare': return restored().file_prepare(operation.space, operation.name, operation.mime, operation.data, operation.now);
    case 'file_upload_headers': return restored().file_upload_headers(operation.blob, operation.now);
    case 'file_commit': return restored().file_commit(operation.space, operation.plan, operation.now, operation.editId);
    case 'living_space_mutate': return restored().living_space_mutate(operation.space, operation.command, operation.now, operation.editId);
    case 'page_apply': return restored().page_apply(operation.request, operation.now);
    case 'page_create': return restored().page_create(operation.space, operation.title, operation.now, operation.editId);
    case 'living_space_home': {
      restored().set_living_space_enabled(true);
      return restored().living_space_command(operation.space, JSON.stringify({ action: 'home' }));
    }
    case 'receive': return restored().receive(operation.bytes, operation.now);
    case 'queue_message': return restored().queue_message(operation.space, operation.text, operation.now);
    case 'queue_post': return restored().queue_post(operation.space, operation.text, operation.replyTo || undefined, operation.thread, operation.forwarded, operation.now);
    case 'queue_annotation': return restored().queue_annotation(operation.space, operation.target, operation.note, operation.now);
    case 'mark_read': return restored().mark_read(operation.space);
    case 'queue_failed_file': return restored().queue_failed_file(operation.space, operation.doc, operation.now, operation.reason, operation.chunks);
    case 'local_chunk': return restored().local_chunk(operation.blob);
    case 'fail_outbox': return restored().fail_outbox(operation.client_id, operation.reason);
    case 'queue_file': return restored().queue_file(operation.space, operation.doc, operation.now);
    case 'seal_file': return core.seal_file(operation.name, operation.mime, operation.bytes, operation.thumb);
    case 'open_chunk': return core.open_chunk(operation.chunk, operation.sealed);
    case 'blob_put_message': return core.blob_put_message(operation.sha, operation.ts, operation.relay);
    case 'queue_join': return restored().queue_join(operation.space, operation.role, operation.code, operation.now);
    default: { const exhaustive: never = operation; throw new Error(`Unknown operation ${exhaustive}`); }
  }
}

let ready: Promise<unknown> | undefined;
let nonce = 0;
self.onmessage = (event: MessageEvent<unknown>) => {
  const data = object(event.data);
  if (data.kind === 'init') {
    if (!(data.bridge instanceof SharedArrayBuffer) || ready) throw new Error('Invalid signing bridge.');
    const bridge = data.bridge;
    const control = new Int32Array(bridge, 0, 4);
    globalThis.zoenSign = (payload) => {
      Atomics.store(control, 1, ++nonce);
      Atomics.store(control, 0, 0);
      self.postMessage({ kind: 'sign', payload: Uint8Array.from(payload), nonce });
      const result = Atomics.wait(control, 0, 0, 15_000);
      if (result === 'timed-out' || Atomics.load(control, 0) !== 1) throw new Error('Browser device signing failed.');
      return Uint8Array.from(new Uint8Array(bridge, 16, 64));
    };
    const wasmUrl = new URL(string(data.wasmUrl));
    if (wasmUrl.origin !== new URL(self.location.href).origin) throw new Error('Invalid browser core origin.');
    ready = init({ module_or_path: fetch(wasmUrl, { integrity: ZOEN_WASM_INTEGRITY }) }).then(() => self.postMessage({ kind: 'ready' })).catch((error: unknown) => {
      self.postMessage({ kind: 'error', id: 0, message: message(error) });
      throw error;
    });
    return;
  }
  const id = integer(data.id);
  void ready?.then(() => {
    try {
      const result = run(parseOperation(data.operation));
      self.postMessage({ kind: 'result', id, result });
    } catch (error) { self.postMessage({ kind: 'error', id, message: message(error) }); }
  });
};
