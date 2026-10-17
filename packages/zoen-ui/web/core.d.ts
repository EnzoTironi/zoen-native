declare module '*/core/zoen_web_core.js' {
  export class BrowserPrimary {
    constructor(device:string,relay:string,name:string);
    static restore(seed:Uint8Array,account:string,device:string):BrowserPrimary;
    account():string;
    authority_bytes():Uint8Array;
    bind_message(proof:string):Uint8Array;
    finish_bind(proof:string,signature:Uint8Array):string;
    free():void;
  }
  export default function init(options: { module_or_path: Promise<Response> }): Promise<unknown>;
  export function create_link(device: string, relay: string): string;
  export function complete_link(pending: string, sealed: Uint8Array): string;
  export function hello(identity: string, device: string, cert: string): Uint8Array;
  export function auth_message(nonce: string, relay: string): Uint8Array;
  export function device_cert_message(device: string): Uint8Array;
  export function ping():Uint8Array;
  export function auth(signature: string): Uint8Array;
  export function request(id: number, kind: string, value: string): Uint8Array;
  export function decode_server(bytes: Uint8Array): string;
  export function seal_file(name: string, mime: string, bytes: Uint8Array, thumb: Uint8Array): Uint8Array;
  export function open_chunk(chunk: string, sealed: Uint8Array): Uint8Array;
  export function blob_put_message(sha: string, ts: number, relay: string): Uint8Array;
  export class BrowserCore {
    constructor(account: string, snapshot: string);
    free(): void;
    view(): string;
    bootstrap(): string;
    receive(bytes: Uint8Array, now: number): string;
    queue_message(space: string, text: string, now: number): string;
    queue_post(space: string, text: string, replyTo: string | undefined, thread: boolean, forwarded: boolean, now: number): string;
    queue_annotation(space: string, target: string, note: string, now: number): string;
    mark_read(space: string): string;
    fail_outbox(client_id: string, reason: string): string;
    queue_file(space: string, doc: string, now: number): string;
    queue_file_record(space: string, doc: string, now: number): string;
    claim_clock(value:string):string;
    maintenance(now:number):string;
    membership_request(request:string, now:number):string;
    document_request(request: string, now: number): string;
    agent_request(request: string, now: number): string;
    integrity_request(request: string): string;
    profile_request(request: string, now: number): string;
    profile_save(request: string, bytes: Uint8Array, now: number): string;
    profile_photo_framed(identity: string, expected: string): Uint8Array;
    profile_photo_received(identity: string, expected: string, sealed: Uint8Array): string;
    handle_profile(handle:string):string;
    handle_confirmed(profile:string,expected:string):string;
    link_prepare(primary:BrowserPrimary,code:string,now:number):string;
    link_request(request:string,now:number):string;
    history_status():string;
    history_start(now:number):string;
    history_manifest(sealed:Uint8Array, now:number):string;
    history_chunk(index:number, sealed:Uint8Array, now:number):string;
    history_finish(now:number):string;
    history_cleaned(now:number):string;
    app_request(request: string, now: number, locale: string): string;
    directory_profiles(profiles: string): string;
    typing_frame(space: string, typing: boolean): Uint8Array;
    document_uploads(): string;
    document_uploaded(blobs: string): string;
    document_cache_blob(reference: string, bytes: Uint8Array): string;
    queue_failed_file(space: string, doc: string, now: number, reason: string, chunks: string): string;
    local_chunk(blob: string): Uint8Array;
    queue_join(space: string, role: string, code: string, now: number): string;
  }
}
