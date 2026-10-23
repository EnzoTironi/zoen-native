import { array, boolean, integer, object, string, message } from './boundary.ts';
import { socketUrl } from './session.ts';
import type { CoreWorker } from './worker-client.ts';
import type { BrowserVault } from './vault.ts';

export type ConnectionState =
  | { kind: 'connecting' }
  | { kind: 'authenticating' }
  | { kind: 'ready' }
  | { kind: 'reconnecting'; attempt: number }
  | { kind: 'stopped' }
  | { kind: 'failed'; message: string };

export type Login = { identity: string; device: string; cert: string; relay: string };

type Reply = {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
};
export type TransportCore = Pick<CoreWorker, 'bytes' | 'text'>;
export type TransportVault = Pick<BrowserVault, 'sign'>;

export function parseFrame(json: string) {
  const value = object(JSON.parse(json));
  const kind = string(value.kind);
  switch (kind) {
    case 'challenge': {
      const capabilities =
        value.capabilities === undefined ? [] : array(value.capabilities).map(string);
      const sample = value.server_time_ms == null ? '' : string(value.server_time_ms);
      if (
        sample &&
        (!/^[0-9]+$/.test(sample) || BigInt(sample) < 1n || BigInt(sample) > 0xffffffffffffn)
      )
        throw new Error('Invalid relay clock.');
      return {
        kind,
        nonce: string(value.nonce),
        relay: string(value.relay),
        protocol: integer(value.protocol),
        capabilities,
        server_time_ms: sample,
      };
    }
    case 'pong': {
      const sample = value.server_time_ms == null ? '' : string(value.server_time_ms);
      if (
        sample &&
        (!/^[0-9]+$/.test(sample) || BigInt(sample) < 1n || BigInt(sample) > 0xffffffffffffn)
      )
        throw new Error('Invalid relay clock.');
      return { kind, server_time_ms: sample };
    }
    case 'ready':
      return { kind, identity: string(value.identity), registered: boolean(value.registered) };
    case 'response':
      return { kind, id: integer(value.id), value: value.value };
    case 'request_error':
      return { kind, id: integer(value.id), message: string(value.message) };
    case 'error':
      return { kind, code: string(value.code), message: string(value.message) };
    case 'joined':
      return { kind, space: string(value.space) };
    case 'accepted': {
      const seq = string(value.seq);
      if (!/^[0-9]+$/.test(seq) || BigInt(seq) > 18446744073709551615n)
        throw new Error('Invalid acknowledgement sequence.');
      return { kind, space: string(value.space), client_id: string(value.client_id), seq };
    }
    case 'ephemeral':
      return {
        kind,
        space: string(value.space),
        from: string(value.from),
        event: string(value.event),
        detail: string(value.detail),
      };
    case 'presence':
      return { kind, identity: string(value.identity), online: boolean(value.online) };
    case 'rejected':
      return {
        kind,
        space: string(value.space),
        client_id: string(value.client_id),
        reason: string(value.reason),
        permanent: boolean(value.permanent),
      };
    case 'key_packages_low':
      return { kind, device: string(value.device), remaining: integer(value.remaining) };
    case 'event':
    case 'sync_done':
    case 'other':
      return { kind };
    default:
      throw new Error('Unrecognized relay frame.');
  }
}
export type RelayFrame = ReturnType<typeof parseFrame>;

export class AuthenticatedTransport {
  private socket: WebSocket | undefined;
  private reconnect: ReturnType<typeof setTimeout> | undefined;
  private handshake: ReturnType<typeof setTimeout> | undefined;
  private heartbeat: ReturnType<typeof setTimeout> | undefined;
  private pongTimeout: ReturnType<typeof setTimeout> | undefined;
  private claimReceipts = false;
  private attempt = 0;
  private generation = 0;
  private requestId = 0;
  private stopped = true;
  private ready = false;
  private readonly pending = new Map<number, Reply>();
  /** The relay's signed name from its challenge: uploads are signed for it. */
  relayName = '';
  claimClock = '';
  private readonly core: TransportCore;
  private readonly vault: TransportVault;
  private readonly login: Login;
  private readonly stateChanged: (state: ConnectionState) => void;
  private readonly received: (frame: RelayFrame, raw: Uint8Array) => Promise<void>;

  constructor(
    core: TransportCore,
    vault: TransportVault,
    login: Login,
    stateChanged: (state: ConnectionState) => void,
    received: (frame: RelayFrame, raw: Uint8Array) => Promise<void>,
  ) {
    this.core = core;
    this.vault = vault;
    this.login = login;
    this.stateChanged = stateChanged;
    this.received = received;
  }

  start(): void {
    if (!this.stopped) return;
    this.stopped = false;
    this.connect();
  }

  private connect(): void {
    if (this.stopped) return;
    const generation = ++this.generation;
    const socket = new WebSocket(socketUrl(this.login.relay));
    socket.binaryType = 'arraybuffer';
    this.socket = socket;
    this.ready = false;
    this.claimClock = '';
    this.claimReceipts = false;
    let phase: 'hello' | 'challenge' | 'auth' | 'ready' = 'hello';
    let incoming: Promise<void> = Promise.resolve();
    this.stateChanged(
      this.attempt === 0 ? { kind: 'connecting' } : { kind: 'reconnecting', attempt: this.attempt },
    );
    this.handshake = setTimeout(() => socket.close(4000, 'Authentication timeout'), 15_000);
    socket.onopen = () => {
      void this.core
        .bytes({ kind: 'hello', ...this.login })
        .then((bytes) => {
          if (generation !== this.generation || socket.readyState !== WebSocket.OPEN) return;
          phase = 'challenge';
          socket.send(bytes);
          this.stateChanged({ kind: 'authenticating' });
        })
        .catch((error: unknown) => {
          if (
            generation !== this.generation ||
            this.stopped ||
            socket.readyState !== WebSocket.OPEN
          )
            return;
          this.fatal(message(error));
        });
    };
    socket.onmessage = (event: MessageEvent<unknown>) => {
      incoming = incoming
        .then(async () => {
          if (generation !== this.generation || this.stopped) return;
          if (!(event.data instanceof ArrayBuffer) || event.data.byteLength > 1_048_576) {
            throw new Error('The relay sent an invalid binary frame.');
          }
          const raw = new Uint8Array(event.data);
          const frame = parseFrame(await this.core.text({ kind: 'decode_server', bytes: raw }));
          if (
            generation !== this.generation ||
            this.stopped ||
            socket.readyState !== WebSocket.OPEN
          )
            return;
          if (frame.kind === 'error') {
            if (['unauthorized', 'upgrade_required'].includes(frame.code)) {
              this.fatal(frame.message);
              return;
            }
            throw new Error(frame.message);
          }
          if (phase === 'challenge' && frame.kind === 'challenge') {
            if (frame.protocol < 4) {
              this.fatal('This relay needs protocol 4 before a browser can pair.');
              return;
            }
            phase = 'auth';
            this.relayName = frame.relay;
            this.claimReceipts = frame.capabilities.includes('key-package-claim-receipts');
            this.claimClock = this.claimReceipts ? frame.server_time_ms : '';
            const payload = await this.core.bytes({
              kind: 'auth_message',
              nonce: frame.nonce,
              relay: frame.relay,
            });
            const signature = await this.vault.sign(payload);
            const signed = Array.from(signature, (b) => b.toString(16).padStart(2, '0')).join('');
            if (generation !== this.generation || socket.readyState !== WebSocket.OPEN) return;
            const authentication = await this.core.bytes({ kind: 'auth', signature: signed });
            if (
              generation !== this.generation ||
              this.stopped ||
              socket.readyState !== WebSocket.OPEN
            )
              return;
            socket.send(authentication);
            return;
          }
          if (phase === 'auth' && frame.kind === 'ready') {
            if (frame.identity !== this.login.identity)
              throw new Error('The relay authenticated a different identity.');
            phase = 'ready';
            this.ready = true;
            this.attempt = 0;
            clearTimeout(this.handshake);
            this.stateChanged({ kind: 'ready' });
            this.schedulePing(generation);
            // Dispatch without awaiting: a Ready handler may itself request a reply.
            void this.received(frame, raw).catch((error: unknown) => {
              if (
                generation !== this.generation ||
                this.stopped ||
                socket.readyState !== WebSocket.OPEN
              )
                return;
              this.fatal(message(error));
            });
            return;
          }
          // Native relays may announce presence while Ready is being delivered.
          if (phase === 'auth' && (frame.kind === 'other' || frame.kind === 'presence')) return;
          if (phase !== 'ready')
            throw new Error('The relay sent data before authentication completed.');
          if (frame.kind === 'pong') {
            clearTimeout(this.pongTimeout);
            this.claimClock = this.claimReceipts ? frame.server_time_ms : '';
            this.schedulePing(generation);
          }
          if (frame.kind === 'response' || frame.kind === 'request_error') {
            const request = this.pending.get(frame.id);
            if (!request) {
              await this.received(frame, raw);
              return;
            }
            clearTimeout(request.timer);
            this.pending.delete(frame.id);
            if (frame.kind === 'request_error') request.reject(new Error(frame.message));
            else request.resolve(frame.value);
            return;
          }
          await this.received(frame, raw);
        })
        .catch((error: unknown) => {
          if (
            generation !== this.generation ||
            this.stopped ||
            socket.readyState !== WebSocket.OPEN
          )
            return;
          this.fatal(message(error));
        });
    };
    socket.onerror = () => {
      /* The close event owns retries and request cleanup. */
    };
    socket.onclose = (event) => this.disconnected(generation, event.code);
  }

  private disconnected(generation: number, code: number): void {
    if (generation !== this.generation) return;
    this.ready = false;
    // Retire synchronously. The public WebSocket close event can be delayed;
    // pending work must not keep the previous authenticated owner until then.
    ++this.generation;
    this.socket = undefined;
    clearTimeout(this.handshake);
    clearTimeout(this.heartbeat);
    clearTimeout(this.pongTimeout);
    this.rejectPending(new Error('Connection closed.'));
    if (this.stopped) return;
    if (code === 1008) {
      this.fatal('This browser device is no longer authorized. Pair it again from your phone.');
      return;
    }
    const attempt = ++this.attempt;
    this.stateChanged({ kind: 'reconnecting', attempt });
    const delay =
      Math.min(30_000, 700 * 2 ** Math.min(attempt - 1, 6)) * (0.8 + Math.random() * 0.4);
    this.reconnect = setTimeout(() => this.connect(), delay);
  }

  /** Retire this owner and use the same normal reconnect/backoff path. */
  reconcileTimeout(): void {
    if (this.stopped || !this.ready) return;
    const socket = this.socket;
    this.disconnected(this.generation, 4000);
    try {
      socket?.close(4000, 'Cursor reconciliation timeout');
    } catch {
      /* Logical retirement does not depend on a later close event. */
    }
  }

  private schedulePing(generation: number): void {
    clearTimeout(this.heartbeat);
    this.heartbeat = setTimeout(() => {
      if (this.stopped || !this.ready || generation !== this.generation) return;
      void this.core
        .bytes({ kind: 'ping' })
        .then((frame) => {
          if (this.stopped || !this.ready || generation !== this.generation) return;
          this.send(frame);
          this.pongTimeout = setTimeout(() => {
            if (generation === this.generation) this.socket?.close(4000, 'Heartbeat timeout');
          }, 15_000);
        })
        .catch(() => {
          if (generation === this.generation) this.socket?.close(4000, 'Heartbeat failed');
        });
    }, 30_000);
  }

  async request(kind: string, value = ''): Promise<unknown> {
    if (!this.ready) throw new Error('The relay is reconnecting. Try again when connected.');
    const id = ++this.requestId;
    const generation = this.generation;
    const frame = await this.core.bytes({ kind: 'request', requestId: id, op: kind, value });
    if (this.stopped || !this.ready || generation !== this.generation)
      throw new Error('A conexão mudou. Tente de novo.');
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error('The relay request timed out.'));
      }, 15_000);
      this.pending.set(id, { resolve, reject, timer });
      try {
        this.send(frame);
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        reject(new Error(message(error)));
      }
    });
  }

  send(frame: Uint8Array): void {
    if (!this.ready || this.socket?.readyState !== WebSocket.OPEN)
      throw new Error('The relay is not connected.');
    if (this.socket.bufferedAmount > 1_048_576)
      throw new Error('The connection is busy. Try again shortly.');
    this.socket.send(Uint8Array.from(frame));
  }

  /** A durable core snapshot owns these bytes. A close/backpressure race must
   * reconnect and bootstrap that journal instead of terminating the account. */
  sendRetained(frame: Uint8Array): void {
    try {
      this.send(frame);
    } catch {
      if (this.stopped) return;
      try {
        this.socket?.close(4000, 'Retry retained encrypted state');
      } catch {
        /* A socket already closing will still deliver its close event. */
      }
    }
  }

  private rejectPending(error: Error): void {
    for (const request of this.pending.values()) {
      clearTimeout(request.timer);
      request.reject(error);
    }
    this.pending.clear();
  }

  private fatal(reason: string): void {
    this.stop();
    this.stateChanged({ kind: 'failed', message: reason });
  }

  stop(): void {
    this.stopped = true;
    this.ready = false;
    ++this.generation;
    clearTimeout(this.reconnect);
    clearTimeout(this.handshake);
    clearTimeout(this.heartbeat);
    clearTimeout(this.pongTimeout);
    this.socket?.close(1000, 'Browser closed');
    this.socket = undefined;
    this.rejectPending(new Error('Connection stopped.'));
    this.stateChanged({ kind: 'stopped' });
  }
}
