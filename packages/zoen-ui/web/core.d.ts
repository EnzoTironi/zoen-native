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
    receive(bytes: Uint8Array, now: number): string;
    queue_message(space: string, text: string, now: number): string;
    queue_post(space: string, text: string, replyTo: string | undefined, thread: boolean, forwarded: boolean, alsoInChat: boolean, now: number): string;
    queue_annotation(space: string, target: string, note: string, now: number): string;
    mark_read(space: string): string;
    set_thread_follow(space: string, root: string, follow: boolean | undefined, now: number): string;
    mark_thread_read(space: string, root: string, now: number): string;
    queue_join(space: string, role: string, code: string, now: number): string;
  }
}
