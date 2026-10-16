import { integer, message, object, string } from './boundary.ts';
import type { BrowserVault } from './vault.ts';

export type CoreOperation =
  | { kind: 'create_link'; device: string; relay: string }
  | { kind: 'complete_link'; pending: string; sealed: Uint8Array }
  | { kind: 'hello'; identity: string; device: string; cert: string }
  | { kind: 'auth_message'; nonce: string; relay: string }
  | { kind: 'device_cert_message'; device: string }
  | { kind: 'auth'; signature: string }
  | { kind: 'request'; requestId: number; op: string; value: string }
  | { kind: 'decode_server'; bytes: Uint8Array }
  | { kind: 'restore'; account: string; snapshot: string }
  | { kind: 'bootstrap' }
  | { kind: 'receive'; bytes: Uint8Array; now: number }
  | { kind: 'queue_message'; space: string; text: string; now: number }
  | { kind: 'queue_post'; space: string; text: string; replyTo: string; thread: boolean; forwarded: boolean; now: number }
  | { kind: 'queue_annotation'; space: string; target: string; note: string; now: number }
  | { kind: 'mark_read'; space: string }
  | { kind: 'queue_file'; space: string; doc: string; now: number }
  | { kind: 'queue_failed_file'; space: string; doc: string; now: number; reason: string; chunks: string }
  | { kind: 'local_chunk'; blob: string }
  | { kind: 'fail_outbox'; client_id: string; reason: string }
  | { kind: 'seal_file'; name: string; mime: string; bytes: Uint8Array; thumb: Uint8Array }
  | { kind: 'open_chunk'; chunk: string; sealed: Uint8Array }
  | { kind: 'blob_put_message'; sha: string; ts: number; relay: string }
  | { kind: 'queue_join'; space: string; role: string; code: string; now: number };

declare const ZOEN_WORKER_INTEGRITY: string;
type Awaiting = { resolve: (result: string | Uint8Array) => void; reject: (error: Error) => void; timer: ReturnType<typeof setTimeout> };

export class CoreWorker {
  private readonly worker: Worker;
  private readonly bridge = new SharedArrayBuffer(128);
  private readonly waiting = new Map<number, Awaiting>();
  private sequence = 0;
  private disposed = false;
  private readonly ready: Promise<void>;
  private loading: ReturnType<typeof setTimeout> | undefined;
  private rejectReady: (error: Error) => void = () => {};

  static async create(vault: BrowserVault): Promise<CoreWorker> {
    const response = await fetch(new URL('/worker.js', window.location.origin), { integrity: ZOEN_WORKER_INTEGRITY });
    if (!response.ok) throw new Error('The encrypted browser core could not be downloaded.');
    const blob = URL.createObjectURL(new Blob([await response.text()], { type: 'text/javascript' }));
    const core = new CoreWorker(new Worker(blob, { type: 'module', name: 'zoen-mls' }), vault);
    try { await core.ready; return core; }
    catch (error) { core.close(); throw error; }
    finally { URL.revokeObjectURL(blob); }
  }

  private constructor(worker: Worker, vault: BrowserVault) {
    this.worker = worker;
    this.ready = new Promise((resolve, reject) => {
      this.rejectReady = reject;
      this.loading = setTimeout(() => this.close(new Error('The encrypted browser core timed out.')), 30_000);
      this.worker.onerror = (event) => {
        const error = new Error(event.message || 'The encrypted browser core did not load.');
        this.close(error);
      };
      this.worker.onmessage = (event: MessageEvent<unknown>) => {
        try {
          const data = object(event.data);
          switch (data.kind) {
            case 'ready': clearTimeout(this.loading); resolve(); return;
            case 'sign': {
              if (!(data.payload instanceof Uint8Array)) throw new Error('Invalid signing request.');
              const control = new Int32Array(this.bridge, 0, 4);
              const nonce = integer(data.nonce);
              void vault.sign(data.payload).then((signature) => {
                if (Atomics.load(control, 1) !== nonce || this.disposed) return;
                if (signature.length !== 64) throw new Error('Invalid Ed25519 signature length.');
                new Uint8Array(this.bridge, 16, 64).set(signature);
                Atomics.store(control, 0, 1);
                Atomics.notify(control, 0);
              }).catch(() => {
                if (Atomics.load(control, 1) !== nonce) return;
                Atomics.store(control, 0, -1);
                Atomics.notify(control, 0);
              });
              return;
            }
            case 'result': {
              const id = integer(data.id);
              const pending = this.waiting.get(id);
              if (!pending) return;
              this.waiting.delete(id);
              clearTimeout(pending.timer);
              if (data.result instanceof Uint8Array || typeof data.result === 'string') pending.resolve(data.result);
              else pending.reject(new Error('Invalid worker reply.'));
              return;
            }
            case 'error': {
              const error = new Error(string(data.message));
              if (data.id === 0) { this.close(error); return; }
              const id = integer(data.id);
              this.waiting.get(id)?.reject(error);
              clearTimeout(this.waiting.get(id)?.timer);
              this.waiting.delete(id);
              return;
            }
            default: throw new Error('Unknown worker message.');
          }
        } catch (error) { this.close(new Error(message(error))); }
      };
      this.worker.postMessage({ kind: 'init', bridge: this.bridge, locale: document.documentElement.lang || navigator.language,
        wasmUrl: new URL('/core/zoen_web_core_bg.wasm', window.location.origin).toString() });
    });
  }

  private fail(error: Error): void {
    for (const request of this.waiting.values()) { clearTimeout(request.timer); request.reject(error); }
    this.waiting.clear();
  }

  private async call(operation: CoreOperation): Promise<string | Uint8Array> {
    await this.ready;
    if (this.disposed) throw new Error('Browser core is closed.');
    const id = ++this.sequence;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => this.close(new Error('The encrypted browser operation timed out.')), 30_000);
      this.waiting.set(id, { resolve, reject, timer });
      this.worker.postMessage({ id, operation });
    });
  }

  async text(operation: CoreOperation): Promise<string> {
    const result = await this.call(operation);
    if (typeof result !== 'string') throw new Error('Expected browser core text.');
    return result;
  }

  async bytes(operation: CoreOperation): Promise<Uint8Array<ArrayBuffer>> {
    const result = await this.call(operation);
    if (!(result instanceof Uint8Array)) throw new Error('Expected browser core bytes.');
    return Uint8Array.from(result);
  }

  close(error = new Error('Browser core is closed.')): void {
    if (this.disposed) return;
    this.disposed = true;
    clearTimeout(this.loading);
    this.rejectReady(error);
    const control = new Int32Array(this.bridge, 0, 4);
    Atomics.store(control, 0, -1);
    Atomics.notify(control, 0);
    this.worker.terminate();
    this.fail(error);
  }
}
