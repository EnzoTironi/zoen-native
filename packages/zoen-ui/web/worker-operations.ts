// Pure validation of worker operations; no signing, client or session owner.
import { bytes, integer, object, string } from './boundary.ts';
import type { CoreOperation } from './worker-client.ts';

export function parseOperation(input: unknown): CoreOperation {
  const value = object(input);
  switch (value.kind) {
    case 'create_link':
      return { kind: 'create_link', device: string(value.device), relay: string(value.relay) };
    case 'complete_link':
      return { kind: 'complete_link', pending: string(value.pending), sealed: bytes(value.sealed) };
    case 'primary_prepare':
      return {
        kind: 'primary_prepare',
        device: string(value.device),
        relay: string(value.relay),
        name: string(value.name),
      };
    case 'primary_restore':
      return {
        kind: 'primary_restore',
        device: string(value.device),
        account: string(value.account),
        authority: string(value.authority),
      };
    case 'primary_bind_message':
      return { kind: 'primary_bind_message', proof: string(value.proof) };
    case 'primary_finish_bind':
      return {
        kind: 'primary_finish_bind',
        proof: string(value.proof),
        signature: bytes(value.signature),
      };
    case 'hello':
      return {
        kind: 'hello',
        identity: string(value.identity),
        device: string(value.device),
        cert: string(value.cert),
      };
    case 'auth_message':
      return { kind: 'auth_message', nonce: string(value.nonce), relay: string(value.relay) };
    case 'device_cert_message':
      return { kind: 'device_cert_message', device: string(value.device) };
    case 'ping':
      return { kind: 'ping' };
    case 'auth':
      return { kind: 'auth', signature: string(value.signature) };
    case 'request':
      return {
        kind: 'request',
        requestId: integer(value.requestId),
        op: string(value.op),
        value: string(value.value),
      };
    case 'decode_server':
      return { kind: 'decode_server', bytes: bytes(value.bytes) };
    case 'restore':
      return { kind: 'restore', account: string(value.account), snapshot: string(value.snapshot) };
    case 'bootstrap':
      return { kind: 'bootstrap', repairNow: integer(value.repairNow) };
    case 'cursor_repair_config':
      return { kind: 'cursor_repair_config' };
    case 'reconcile':
      return { kind: 'reconcile', now: integer(value.now) };
    case 'claim_clock':
      return { kind: 'claim_clock', value: string(value.value) };
    case 'maintenance':
      return { kind: 'maintenance', now: integer(value.now) };
    case 'membership_request':
      return {
        kind: 'membership_request',
        request: string(value.request),
        now: integer(value.now),
      };
    case 'receive':
      return {
        kind: 'receive',
        bytes: bytes(value.bytes),
        now: integer(value.now),
        repairNow: integer(value.repairNow),
      };
    case 'queue_message':
      return {
        kind: 'queue_message',
        space: string(value.space),
        text: string(value.text),
        now: integer(value.now),
      };
    case 'queue_post':
      return {
        kind: 'queue_post',
        space: string(value.space),
        text: string(value.text),
        replyTo: string(value.replyTo),
        thread: value.thread === true,
        forwarded: value.forwarded === true,
        now: integer(value.now),
      };
    case 'queue_annotation':
      return {
        kind: 'queue_annotation',
        space: string(value.space),
        target: string(value.target),
        note: string(value.note),
        now: integer(value.now),
      };
    case 'mark_read':
      return { kind: 'mark_read', space: string(value.space) };
    case 'attachment_producer':
      return { kind: 'attachment_producer', request: string(value.request) };
    case 'queue_attachment':
      return {
        kind: 'queue_attachment',
        request: string(value.request),
        framed: bytes(value.framed),
        now: integer(value.now),
      };
    case 'queue_attachment_batch':
      return {
        kind: 'queue_attachment_batch',
        request: string(value.request),
        framed: bytes(value.framed),
        now: integer(value.now),
      };
    case 'queue_file_record':
      return {
        kind: 'queue_file_record',
        space: string(value.space),
        doc: string(value.doc),
        now: integer(value.now),
      };
    case 'queue_file':
      return {
        kind: 'queue_file',
        space: string(value.space),
        doc: string(value.doc),
        now: integer(value.now),
      };
    case 'queue_failed_file':
      return {
        kind: 'queue_failed_file',
        space: string(value.space),
        doc: string(value.doc),
        now: integer(value.now),
        reason: string(value.reason),
        chunks: string(value.chunks),
      };
    case 'local_chunk':
      return { kind: 'local_chunk', blob: string(value.blob) };
    case 'fail_outbox':
      return {
        kind: 'fail_outbox',
        client_id: string(value.client_id),
        reason: string(value.reason),
      };
    case 'seal_file':
      return {
        kind: 'seal_file',
        name: string(value.name),
        mime: string(value.mime),
        bytes: bytes(value.bytes),
        thumb: bytes(value.thumb),
      };
    case 'open_chunk':
      return { kind: 'open_chunk', chunk: string(value.chunk), sealed: bytes(value.sealed) };
    case 'blob_put_message':
      return {
        kind: 'blob_put_message',
        sha: string(value.sha),
        ts: integer(value.ts),
        relay: string(value.relay),
      };
    case 'document_request':
      return { kind: 'document_request', request: string(value.request), now: integer(value.now) };
    case 'agent_request':
      return { kind: 'agent_request', request: string(value.request), now: integer(value.now) };
    case 'integrity_request':
      return { kind: 'integrity_request', request: string(value.request) };
    case 'profile_request':
      return { kind: 'profile_request', request: string(value.request), now: integer(value.now) };
    case 'profile_save':
      return {
        kind: 'profile_save',
        request: string(value.request),
        bytes: bytes(value.bytes),
        now: integer(value.now),
      };
    case 'profile_photo_framed':
      return {
        kind: 'profile_photo_framed',
        identity: string(value.identity),
        expected: string(value.expected),
      };
    case 'profile_photo_received':
      return {
        kind: 'profile_photo_received',
        identity: string(value.identity),
        expected: string(value.expected),
        sealed: bytes(value.sealed),
      };
    case 'handle_profile':
      return { kind: 'handle_profile', handle: string(value.handle) };
    case 'handle_confirmed':
      return {
        kind: 'handle_confirmed',
        profile: string(value.profile),
        expected: string(value.expected),
      };
    case 'link_prepare':
      return { kind: 'link_prepare', code: string(value.code), now: integer(value.now) };
    case 'link_request':
      return { kind: 'link_request', request: string(value.request), now: integer(value.now) };
    case 'history_status':
      return { kind: 'history_status' };
    case 'history_start':
    case 'history_finish':
    case 'history_cleaned':
      return { kind: value.kind, now: integer(value.now) };
    case 'history_manifest':
      return { kind: 'history_manifest', sealed: bytes(value.sealed), now: integer(value.now) };
    case 'history_chunk':
      return {
        kind: 'history_chunk',
        index: integer(value.index),
        sealed: bytes(value.sealed),
        now: integer(value.now),
      };
    case 'app_request':
      return { kind: 'app_request', request: string(value.request), now: integer(value.now) };
    case 'directory_profiles':
      return { kind: 'directory_profiles', profiles: string(value.profiles) };
    case 'typing_frame':
      return { kind: 'typing_frame', space: string(value.space), typing: value.typing === true };
    case 'document_uploads':
      return { kind: 'document_uploads' };
    case 'document_uploaded':
      return { kind: 'document_uploaded', blobs: string(value.blobs) };
    case 'document_cache_blob':
      return {
        kind: 'document_cache_blob',
        reference: string(value.reference),
        bytes: bytes(value.bytes),
      };
    case 'queue_join':
      return {
        kind: 'queue_join',
        space: string(value.space),
        role: string(value.role),
        code: string(value.code),
        now: integer(value.now),
      };
    default:
      throw new Error('Invalid browser core operation.');
  }
}
