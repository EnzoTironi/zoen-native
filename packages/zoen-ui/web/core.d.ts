declare module '*/core/zoen_web_core.js' {
  export default function init(options: { module_or_path: Promise<Response> }): Promise<unknown>;
  export function create_link(device: string, relay: string): string;
  export function complete_link(pending: string, sealed: Uint8Array): string;
  export function hello(identity: string, device: string, cert: string): Uint8Array;
  export function auth_message(nonce: string, relay: string): Uint8Array;
  export function device_cert_message(device: string): Uint8Array;
  export function auth(signature: string): Uint8Array;
  export function request(id: number, kind: string, value: string): Uint8Array;
  export function decode_server(bytes: Uint8Array): string;
  export class BrowserCore {
    constructor(account: string, snapshot: string);
    free(): void;
    view(): string;
    bootstrap(): string;
    set_living_space_enabled(enabled: boolean): void;
    living_space_command(space: string, command: string): string;
    living_space_mutate(space: string, command: string, now: number, editId: string): string;
    page(item: string): string;
    page_download_manifest(item: string): string;
    page_blob_supply(item: string, blob: string, data: Uint8Array): void;
    page_apply(request: string, now: number): string;
    page_create(space: string, title: string, now: number, editId: string): string;
    file_manifest(item: string): string;
    file_open(item: string, data: Uint8Array): Uint8Array;
    file_prepare(space: string, name: string, mime: string, data: Uint8Array, now: number): string;
    file_upload_headers(blob: string, now: number): string;
    file_commit(space: string, plan: string, now: number, editId: string): string;
    receive(bytes: Uint8Array, now: number): string;
    queue_message(space: string, text: string, now: number): string;
    queue_post(space: string, text: string, replyTo: string | undefined, thread: boolean, forwarded: boolean, now: number): string;
    queue_annotation(space: string, target: string, note: string, now: number): string;
    mark_read(space: string): string;
    queue_join(space: string, role: string, code: string, now: number): string;
  }
}
