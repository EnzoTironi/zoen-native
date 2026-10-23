import { parseOperation } from './worker-operations.ts';
/// <reference lib="webworker" />
import init, * as core from './core/zoen_web_core.js';
import { integer, message, object, string } from './boundary.ts';
import type { CoreOperation } from './worker-client.ts';
import { sessionLocale, type AppLocale } from './locale.ts';
import { authorityJson, openAuthority, parseAuthority, sealAuthority } from './authority.ts';

declare global {
  var zoenSign: (payload: Uint8Array) => Uint8Array;
}
declare const ZOEN_WASM_INTEGRITY: string;

let locale: AppLocale;
let client: core.BrowserCore | undefined;
let primary: core.BrowserPrimary | undefined;
let primaryEnvelope: string | undefined;
let authorityKey: CryptoKey | undefined;
let messagingAllowed = false;
function restored(): core.BrowserCore {
  if (!client) throw new Error('The encrypted session has not been restored.');
  return client;
}

async function run(operation: CoreOperation): Promise<string | Uint8Array> {
  switch (operation.kind) {
    case 'create_link':
      return core.create_link(operation.device, operation.relay);
    case 'complete_link':
      return core.complete_link(operation.pending, operation.sealed);
    case 'primary_prepare': {
      if (!authorityKey)
        throw new Error('A proteção das chaves deste navegador não está disponível.');
      if (!primary) {
        primary = new core.BrowserPrimary(
          operation.device,
          operation.relay,
          operation.name,
          locale,
        );
      }
      const account = JSON.parse(primary.account()) as {
        identity: string;
        device: string;
        relay: string;
      };
      if (account.device !== operation.device || account.relay !== operation.relay)
        throw new Error('Este navegador já preparou outra conta.');
      if (!primaryEnvelope) {
        const envelope = await sealAuthority(
          primary.authority_bytes(),
          authorityKey,
          account.identity,
          account.device,
        );
        primaryEnvelope = authorityJson(envelope);
      }
      return JSON.stringify({ account, authority: JSON.parse(primaryEnvelope) });
    }
    case 'primary_restore': {
      if (!authorityKey)
        throw new Error('A proteção das chaves deste navegador não está disponível.');
      const envelope = parseAuthority(JSON.parse(operation.authority));
      const seed = await openAuthority(envelope, authorityKey, operation.device);
      try {
        const restored = core.BrowserPrimary.restore(
          seed,
          operation.account,
          operation.device,
          locale,
        );
        const account = JSON.parse(restored.account()) as { identity: string };
        if (account.identity !== envelope.identity) {
          restored.free();
          throw new Error('A autoridade não corresponde à conta salva.');
        }
        primary?.free();
        primary = restored;
        primaryEnvelope = authorityJson(envelope);
        return primary.account();
      } finally {
        seed.fill(0);
      }
    }
    case 'primary_bind_message': {
      if (!primary) throw new Error('Recupere as chaves deste navegador antes de entrar.');
      return primary.bind_message(operation.proof);
    }
    case 'primary_finish_bind': {
      if (!primary) throw new Error('Recupere as chaves deste navegador antes de entrar.');
      return primary.finish_bind(operation.proof, operation.signature);
    }
    case 'hello':
      return core.hello(operation.identity, operation.device, operation.cert);
    case 'auth_message':
      return core.auth_message(operation.nonce, operation.relay);
    case 'device_cert_message':
      return core.device_cert_message(operation.device);
    case 'ping':
      return core.ping();
    case 'auth':
      return core.auth(operation.signature);
    case 'request':
      return core.request(operation.requestId, operation.op, operation.value);
    case 'decode_server':
      return core.decode_server(operation.bytes);
    case 'restore': {
      if (!messagingAllowed)
        throw new Error('Abra o Zoen na página protegida para sincronizar suas conversas.');
      client?.free();
      client = undefined;
      client = new core.BrowserCore(operation.account, operation.snapshot, locale);
      return client.view();
    }
    case 'bootstrap':
      return restored().bootstrap_at(operation.repairNow);
    case 'cursor_repair_config':
      return restored().cursor_repair_config();
    case 'reconcile':
      return restored().reconcile(operation.now);
    case 'receive':
      return restored().receive_at(operation.bytes, operation.now, operation.repairNow);
    case 'queue_message':
      return restored().queue_message(operation.space, operation.text, operation.now);
    case 'queue_post':
      return restored().queue_post(
        operation.space,
        operation.text,
        operation.replyTo || undefined,
        operation.thread,
        operation.forwarded,
        operation.now,
      );
    case 'queue_annotation':
      return restored().queue_annotation(
        operation.space,
        operation.target,
        operation.note,
        operation.now,
      );
    case 'mark_read':
      return restored().mark_read(operation.space);
    case 'queue_failed_file':
      return restored().queue_failed_file(
        operation.space,
        operation.doc,
        operation.now,
        operation.reason,
        operation.chunks,
      );
    case 'local_chunk':
      return restored().local_chunk(operation.blob);
    case 'fail_outbox':
      return restored().fail_outbox(operation.client_id, operation.reason);
    case 'attachment_producer':
      return restored().attachment_producer(operation.request);
    case 'queue_attachment':
      return restored().queue_attachment(operation.request, operation.framed, operation.now);
    case 'queue_attachment_batch':
      return restored().queue_attachment_batch(operation.request, operation.framed, operation.now);
    case 'queue_file_record':
      return restored().queue_file_record(operation.space, operation.doc, operation.now);
    case 'queue_file':
      return restored().queue_file(operation.space, operation.doc, operation.now);
    case 'seal_file':
      return core.seal_file(operation.name, operation.mime, operation.bytes, operation.thumb);
    case 'open_chunk':
      return core.open_chunk(operation.chunk, operation.sealed);
    case 'blob_put_message':
      return core.blob_put_message(operation.sha, operation.ts, operation.relay);
    case 'claim_clock':
      return restored().claim_clock(operation.value);
    case 'maintenance':
      return restored().maintenance(operation.now);
    case 'membership_request':
      return restored().membership_request(operation.request, operation.now);
    case 'document_request':
      return restored().document_request(operation.request, operation.now);
    case 'agent_request':
      return restored().agent_request(operation.request, operation.now);
    case 'integrity_request':
      return restored().integrity_request(operation.request);
    case 'profile_request':
      return restored().profile_request(operation.request, operation.now);
    case 'profile_save':
      return restored().profile_save(operation.request, operation.bytes, operation.now);
    case 'profile_photo_framed':
      return restored().profile_photo_framed(operation.identity, operation.expected);
    case 'profile_photo_received':
      return restored().profile_photo_received(
        operation.identity,
        operation.expected,
        operation.sealed,
      );
    case 'handle_profile':
      return restored().handle_profile(operation.handle);
    case 'handle_confirmed':
      return restored().handle_confirmed(operation.profile, operation.expected);
    case 'link_prepare': {
      if (!primary)
        throw new Error(
          'A chave da conta não está neste navegador. Vincule o aparelho pelo dispositivo principal.',
        );
      return restored().link_prepare(primary, operation.code, operation.now);
    }
    case 'link_request':
      return restored().link_request(operation.request, operation.now);
    case 'history_status':
      return restored().history_status();
    case 'history_start':
      return restored().history_start(operation.now);
    case 'history_manifest':
      return restored().history_manifest(operation.sealed, operation.now);
    case 'history_chunk':
      return restored().history_chunk(operation.index, operation.sealed, operation.now);
    case 'history_finish':
      return restored().history_finish(operation.now);
    case 'history_cleaned':
      return restored().history_cleaned(operation.now);
    case 'app_request':
      return restored().app_request(operation.request, operation.now);
    case 'directory_profiles':
      return restored().directory_profiles(operation.profiles);
    case 'typing_frame':
      return restored().typing_frame(operation.space, operation.typing);
    case 'document_uploads':
      return restored().document_uploads();
    case 'document_uploaded':
      return restored().document_uploaded(operation.blobs);
    case 'document_cache_blob':
      return restored().document_cache_blob(operation.reference, operation.bytes);
    case 'queue_join':
      return restored().queue_join(operation.space, operation.role, operation.code, operation.now);
    default: {
      const exhaustive: never = operation;
      throw new Error(`Unknown operation ${exhaustive}`);
    }
  }
}

let ready: Promise<unknown> | undefined;
let serial: Promise<void> = Promise.resolve();
let nonce = 0;
self.onmessage = (event: MessageEvent<unknown>) => {
  const data = object(event.data);
  if (data.kind === 'init') {
    if (
      ready ||
      (data.bridge !== null &&
        (typeof SharedArrayBuffer === 'undefined' || !(data.bridge instanceof SharedArrayBuffer)))
    )
      throw new Error('Invalid signing bridge.');
    locale = sessionLocale(data.locale);
    const bridge = data.bridge;
    messagingAllowed = bridge !== null;
    if (
      !(data.authorityKey instanceof CryptoKey) ||
      data.authorityKey.type !== 'secret' ||
      data.authorityKey.extractable ||
      data.authorityKey.algorithm.name !== 'AES-GCM'
    )
      throw new Error('Invalid browser authority protection.');
    authorityKey = data.authorityKey;
    const control = bridge === null ? null : new Int32Array(bridge as SharedArrayBuffer, 0, 4);
    globalThis.zoenSign = (payload) => {
      if (!control || !bridge)
        throw new Error('Encrypted messaging requires an isolated browser page.');
      Atomics.store(control, 1, ++nonce);
      Atomics.store(control, 0, 0);
      self.postMessage({ kind: 'sign', payload: Uint8Array.from(payload), nonce });
      const result = Atomics.wait(control, 0, 0, 15_000);
      if (result === 'timed-out' || Atomics.load(control, 0) !== 1)
        throw new Error('Browser device signing failed.');
      return Uint8Array.from(new Uint8Array(bridge as SharedArrayBuffer, 16, 64));
    };
    const wasmUrl = new URL(string(data.wasmUrl));
    if (wasmUrl.origin !== new URL(self.location.href).origin)
      throw new Error('Invalid browser core origin.');
    ready = init({ module_or_path: fetch(wasmUrl, { integrity: ZOEN_WASM_INTEGRITY }) })
      .then(() => self.postMessage({ kind: 'ready', locale }))
      .catch((error: unknown) => {
        self.postMessage({ kind: 'error', id: 0, message: message(error) });
        throw error;
      });
    return;
  }
  const id = integer(data.id);
  void ready?.then(() => {
    serial = serial.then(async () => {
      try {
        const result = await run(parseOperation(data.operation));
        self.postMessage({ kind: 'result', id, result });
      } catch (error) {
        self.postMessage({ kind: 'error', id, message: message(error) });
      }
    });
  });
};
