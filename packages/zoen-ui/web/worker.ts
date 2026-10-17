/// <reference lib="webworker" />
import init, * as core from './core/zoen_web_core.js';
import { bytes, integer, message, object, string } from './boundary.ts';
import type { CoreOperation } from './worker-client.ts';
import {authorityJson,openAuthority,parseAuthority,sealAuthority} from './authority.ts';

declare global {
  var zoenSign: (payload: Uint8Array) => Uint8Array;
}
declare const ZOEN_WASM_INTEGRITY: string;

function parseOperation(input: unknown): CoreOperation {
  const value = object(input);
  switch (value.kind) {
    case 'create_link': return { kind: 'create_link', device: string(value.device), relay: string(value.relay) };
    case 'complete_link': return { kind: 'complete_link', pending: string(value.pending), sealed: bytes(value.sealed) };
    case 'primary_prepare':return {kind:'primary_prepare',device:string(value.device),relay:string(value.relay),name:string(value.name)};
    case 'primary_restore':return {kind:'primary_restore',device:string(value.device),account:string(value.account),authority:string(value.authority)};
    case 'primary_bind_message':return {kind:'primary_bind_message',proof:string(value.proof)};
    case 'primary_finish_bind':return {kind:'primary_finish_bind',proof:string(value.proof),signature:bytes(value.signature)};
    case 'hello': return { kind: 'hello', identity: string(value.identity), device: string(value.device), cert: string(value.cert) };
    case 'auth_message': return { kind: 'auth_message', nonce: string(value.nonce), relay: string(value.relay) };
    case 'device_cert_message': return { kind: 'device_cert_message', device: string(value.device) };
    case 'ping':return {kind:'ping'};
    case 'auth': return { kind: 'auth', signature: string(value.signature) };
    case 'request': return { kind: 'request', requestId: integer(value.requestId), op: string(value.op), value: string(value.value) };
    case 'decode_server': return { kind: 'decode_server', bytes: bytes(value.bytes) };
    case 'restore': return { kind: 'restore', account: string(value.account), snapshot: string(value.snapshot) };
    case 'bootstrap': return { kind: 'bootstrap' };
    case 'claim_clock':return {kind:'claim_clock',value:string(value.value)};
    case 'maintenance':return {kind:'maintenance',now:integer(value.now)};
    case 'membership_request':return {kind:'membership_request',request:string(value.request),now:integer(value.now)};
    case 'receive': return { kind: 'receive', bytes: bytes(value.bytes), now: integer(value.now) };
    case 'queue_message': return { kind: 'queue_message', space: string(value.space), text: string(value.text), now: integer(value.now) };
    case 'queue_post': return { kind: 'queue_post', space: string(value.space), text: string(value.text), replyTo: string(value.replyTo), thread: value.thread === true, forwarded: value.forwarded === true, now: integer(value.now) };
    case 'queue_annotation': return { kind: 'queue_annotation', space: string(value.space), target: string(value.target), note: string(value.note), now: integer(value.now) };
    case 'mark_read': return { kind: 'mark_read', space: string(value.space) };
    case 'queue_file_record': return { kind: 'queue_file_record', space: string(value.space), doc: string(value.doc), now: integer(value.now) };
    case 'queue_file': return { kind: 'queue_file', space: string(value.space), doc: string(value.doc), now: integer(value.now) };
    case 'queue_failed_file': return { kind: 'queue_failed_file', space: string(value.space), doc: string(value.doc), now: integer(value.now), reason: string(value.reason), chunks: string(value.chunks) };
    case 'local_chunk': return { kind: 'local_chunk', blob: string(value.blob) };
    case 'fail_outbox': return { kind: 'fail_outbox', client_id: string(value.client_id), reason: string(value.reason) };
    case 'seal_file': return { kind: 'seal_file', name: string(value.name), mime: string(value.mime), bytes: bytes(value.bytes), thumb: bytes(value.thumb) };
    case 'open_chunk': return { kind: 'open_chunk', chunk: string(value.chunk), sealed: bytes(value.sealed) };
    case 'blob_put_message': return { kind: 'blob_put_message', sha: string(value.sha), ts: integer(value.ts), relay: string(value.relay) };
    case 'document_request': return { kind: 'document_request', request: string(value.request), now: integer(value.now) };
    case 'agent_request': return { kind: 'agent_request', request: string(value.request), now: integer(value.now) };
    case 'integrity_request': return { kind: 'integrity_request', request: string(value.request) };
    case 'profile_request': return { kind: 'profile_request', request: string(value.request), now: integer(value.now) };
    case 'profile_save': return { kind: 'profile_save', request: string(value.request), bytes: bytes(value.bytes), now: integer(value.now) };
    case 'profile_photo_framed': return { kind: 'profile_photo_framed', identity: string(value.identity), expected: string(value.expected) };
    case 'profile_photo_received': return { kind: 'profile_photo_received', identity: string(value.identity), expected: string(value.expected), sealed: bytes(value.sealed) };
    case 'handle_profile':return {kind:'handle_profile',handle:string(value.handle)};
    case 'handle_confirmed':return {kind:'handle_confirmed',profile:string(value.profile),expected:string(value.expected)};
    case 'link_prepare':return {kind:'link_prepare',code:string(value.code),now:integer(value.now)};
    case 'link_request':return {kind:'link_request',request:string(value.request),now:integer(value.now)};
    case 'history_status':return {kind:'history_status'};
    case 'history_start':case 'history_finish':case 'history_cleaned':return {kind:value.kind,now:integer(value.now)};
    case 'history_manifest':return {kind:'history_manifest',sealed:bytes(value.sealed),now:integer(value.now)};
    case 'history_chunk':return {kind:'history_chunk',index:integer(value.index),sealed:bytes(value.sealed),now:integer(value.now)};
    case 'app_request': return { kind: 'app_request', request: string(value.request), now: integer(value.now), locale: string(value.locale) };
    case 'directory_profiles': return {kind:'directory_profiles',profiles:string(value.profiles)};
    case 'typing_frame': return {kind:'typing_frame',space:string(value.space),typing:value.typing===true};
    case 'document_uploads': return { kind: 'document_uploads' };
    case 'document_uploaded': return { kind: 'document_uploaded', blobs: string(value.blobs) };
    case 'document_cache_blob': return { kind: 'document_cache_blob', reference: string(value.reference), bytes: bytes(value.bytes) };
    case 'queue_join': return { kind: 'queue_join', space: string(value.space), role: string(value.role), code: string(value.code), now: integer(value.now) };
    default: throw new Error('Invalid browser core operation.');
  }
}

let client: core.BrowserCore | undefined;
let primary:core.BrowserPrimary|undefined;
let primaryEnvelope:string|undefined;
let authorityKey:CryptoKey|undefined;
let messagingAllowed=false;
function restored(): core.BrowserCore {
  if (!client) throw new Error('The encrypted session has not been restored.');
  return client;
}

async function run(operation: CoreOperation): Promise<string | Uint8Array> {
  switch (operation.kind) {
    case 'create_link': return core.create_link(operation.device, operation.relay);
    case 'complete_link': return core.complete_link(operation.pending, operation.sealed);
    case 'primary_prepare':{
      if(!authorityKey)throw new Error('A proteção das chaves deste navegador não está disponível.');
      if(!primary){primary=new core.BrowserPrimary(operation.device,operation.relay,operation.name);}
      const account=JSON.parse(primary.account()) as {identity:string;device:string;relay:string};
      if(account.device!==operation.device||account.relay!==operation.relay)throw new Error('Este navegador já preparou outra conta.');
      if(!primaryEnvelope){const envelope=await sealAuthority(primary.authority_bytes(),authorityKey,account.identity,account.device);primaryEnvelope=authorityJson(envelope);}
      return JSON.stringify({account,authority:JSON.parse(primaryEnvelope)});
    }
    case 'primary_restore':{
      if(!authorityKey)throw new Error('A proteção das chaves deste navegador não está disponível.');
      const envelope=parseAuthority(JSON.parse(operation.authority));
      const seed=await openAuthority(envelope,authorityKey,operation.device);
      try{
        const restored=core.BrowserPrimary.restore(seed,operation.account,operation.device);
        const account=JSON.parse(restored.account()) as {identity:string};
        if(account.identity!==envelope.identity){restored.free();throw new Error('A autoridade não corresponde à conta salva.');}
        primary?.free();primary=restored;primaryEnvelope=authorityJson(envelope);return primary.account();
      }finally{seed.fill(0);}
    }
    case 'primary_bind_message':{
      if(!primary)throw new Error('Recupere as chaves deste navegador antes de entrar.');
      return primary.bind_message(operation.proof);
    }
    case 'primary_finish_bind':{
      if(!primary)throw new Error('Recupere as chaves deste navegador antes de entrar.');
      return primary.finish_bind(operation.proof,operation.signature);
    }
    case 'hello': return core.hello(operation.identity, operation.device, operation.cert);
    case 'auth_message': return core.auth_message(operation.nonce, operation.relay);
    case 'device_cert_message': return core.device_cert_message(operation.device);
    case 'ping':return core.ping();
    case 'auth': return core.auth(operation.signature);
    case 'request': return core.request(operation.requestId, operation.op, operation.value);
    case 'decode_server': return core.decode_server(operation.bytes);
    case 'restore': {
      if(!messagingAllowed)throw new Error('Abra o Zoen na página protegida para sincronizar suas conversas.');
      client?.free();
      client = undefined;
      client = new core.BrowserCore(operation.account, operation.snapshot);
      return client.view();
    }
    case 'bootstrap': return restored().bootstrap();
    case 'receive': return restored().receive(operation.bytes, operation.now);
    case 'queue_message': return restored().queue_message(operation.space, operation.text, operation.now);
    case 'queue_post': return restored().queue_post(operation.space, operation.text, operation.replyTo || undefined, operation.thread, operation.forwarded, operation.now);
    case 'queue_annotation': return restored().queue_annotation(operation.space, operation.target, operation.note, operation.now);
    case 'mark_read': return restored().mark_read(operation.space);
    case 'queue_failed_file': return restored().queue_failed_file(operation.space, operation.doc, operation.now, operation.reason, operation.chunks);
    case 'local_chunk': return restored().local_chunk(operation.blob);
    case 'fail_outbox': return restored().fail_outbox(operation.client_id, operation.reason);
    case 'queue_file_record': return restored().queue_file_record(operation.space, operation.doc, operation.now);
    case 'queue_file': return restored().queue_file(operation.space, operation.doc, operation.now);
    case 'seal_file': return core.seal_file(operation.name, operation.mime, operation.bytes, operation.thumb);
    case 'open_chunk': return core.open_chunk(operation.chunk, operation.sealed);
    case 'blob_put_message': return core.blob_put_message(operation.sha, operation.ts, operation.relay);
    case 'claim_clock':return restored().claim_clock(operation.value);
    case 'maintenance':return restored().maintenance(operation.now);
    case 'membership_request':return restored().membership_request(operation.request,operation.now);
    case 'document_request': return restored().document_request(operation.request, operation.now);
    case 'agent_request': return restored().agent_request(operation.request, operation.now);
    case 'integrity_request': return restored().integrity_request(operation.request);
    case 'profile_request': return restored().profile_request(operation.request, operation.now);
    case 'profile_save': return restored().profile_save(operation.request, operation.bytes, operation.now);
    case 'profile_photo_framed': return restored().profile_photo_framed(operation.identity, operation.expected);
    case 'profile_photo_received': return restored().profile_photo_received(operation.identity, operation.expected, operation.sealed);
    case 'handle_profile':return restored().handle_profile(operation.handle);
    case 'handle_confirmed':return restored().handle_confirmed(operation.profile,operation.expected);
    case 'link_prepare':{
      if(!primary)throw new Error('A chave da conta não está neste navegador. Vincule o aparelho pelo dispositivo principal.');
      return restored().link_prepare(primary,operation.code,operation.now);
    }
    case 'link_request':return restored().link_request(operation.request,operation.now);
    case 'history_status':return restored().history_status();
    case 'history_start':return restored().history_start(operation.now);
    case 'history_manifest':return restored().history_manifest(operation.sealed,operation.now);
    case 'history_chunk':return restored().history_chunk(operation.index,operation.sealed,operation.now);
    case 'history_finish':return restored().history_finish(operation.now);
    case 'history_cleaned':return restored().history_cleaned(operation.now);
    case 'app_request': return restored().app_request(operation.request, operation.now, operation.locale);
    case 'directory_profiles': return restored().directory_profiles(operation.profiles);
    case 'typing_frame': return restored().typing_frame(operation.space,operation.typing);
    case 'document_uploads': return restored().document_uploads();
    case 'document_uploaded': return restored().document_uploaded(operation.blobs);
    case 'document_cache_blob': return restored().document_cache_blob(operation.reference, operation.bytes);
    case 'queue_join': return restored().queue_join(operation.space, operation.role, operation.code, operation.now);
    default: { const exhaustive: never = operation; throw new Error(`Unknown operation ${exhaustive}`); }
  }
}

let ready: Promise<unknown> | undefined;
let serial:Promise<void>=Promise.resolve();
let nonce = 0;
self.onmessage = (event: MessageEvent<unknown>) => {
  const data = object(event.data);
  if (data.kind === 'init') {
    if (ready || data.bridge!==null && (typeof SharedArrayBuffer==='undefined'||!(data.bridge instanceof SharedArrayBuffer))) throw new Error('Invalid signing bridge.');
    const bridge = data.bridge;
    messagingAllowed=bridge!==null;
    if(!(data.authorityKey instanceof CryptoKey)||data.authorityKey.type!=='secret'||data.authorityKey.extractable||data.authorityKey.algorithm.name!=='AES-GCM')throw new Error('Invalid browser authority protection.');
    authorityKey=data.authorityKey;
    const control = bridge===null ? null : new Int32Array(bridge as SharedArrayBuffer, 0, 4);
    globalThis.zoenSign = (payload) => {
      if(!control||!bridge)throw new Error('Encrypted messaging requires an isolated browser page.');
      Atomics.store(control, 1, ++nonce);
      Atomics.store(control, 0, 0);
      self.postMessage({ kind: 'sign', payload: Uint8Array.from(payload), nonce });
      const result = Atomics.wait(control, 0, 0, 15_000);
      if (result === 'timed-out' || Atomics.load(control, 0) !== 1) throw new Error('Browser device signing failed.');
      return Uint8Array.from(new Uint8Array(bridge as SharedArrayBuffer, 16, 64));
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
  void ready?.then(() => {serial=serial.then(async()=>{
    try {
      const result = await run(parseOperation(data.operation));
      self.postMessage({ kind: 'result', id, result });
    } catch (error) { self.postMessage({ kind: 'error', id, message: message(error) }); }
  });});
};
