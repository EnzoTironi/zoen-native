import { bytes, hex, object, string } from './boundary.ts';
import { parseAccount, parsePending, relayUrl, type LinkedAccount, type PendingLink, type StoredSession } from './session.ts';
import { AuthenticatedTransport, type ConnectionState } from './transport.ts';
import type { CoreWorker } from './worker-client.ts';
import type { BrowserVault } from './vault.ts';

export type PairingState =
  | { kind: 'starting' }
  | { kind: 'waiting'; pending: PendingLink; connection: ConnectionState }
  | { kind: 'linked'; account: LinkedAccount }
  | { kind: 'failed'; message: string };

type PickupTransport = Pick<AuthenticatedTransport, 'request' | 'start' | 'stop'>;
type TransportFactory = (state: (state: ConnectionState) => void, ready: () => Promise<void>, login: {
  identity: string; device: string; cert: string; relay: string;
}) => PickupTransport;
const EXPIRED = 'O prazo para conectar acabou. Tente de novo para gerar outro código.';
const TTL = 15 * 60_000;

export class Pairing {
  private transport: PickupTransport | undefined;
  private poll: ReturnType<typeof setTimeout> | undefined;
  private expiry: ReturnType<typeof setTimeout> | undefined;
  private canceled = false;
  private fetching = false;
  private pending: PendingLink | undefined;
  private connection: ConnectionState = { kind: 'connecting' };

  private readonly vault: Pick<BrowserVault, 'device' | 'sign' | 'save'>;
  private readonly core: Pick<CoreWorker, 'text' | 'bytes'>;
  private readonly changed: (state: PairingState) => void;
  private readonly factory: TransportFactory | undefined;

  constructor(vault: Pick<BrowserVault, 'device' | 'sign' | 'save'>,
    core: Pick<CoreWorker, 'text' | 'bytes'>, changed: (state: PairingState) => void,
    factory?: TransportFactory) {
    this.vault = vault; this.core = core; this.changed = changed; this.factory = factory;
  }

  async start(relay: string, stored: StoredSession): Promise<void> {
    if (this.canceled) return;
    this.changed({ kind: 'starting' });
    try {
      const device = await this.vault.device();
      if (this.canceled) return;
      // Legacy pending records have no trustworthy creation time: replace them.
      const pending = stored.kind === 'pending' && stored.pending.expires_at_ms > Date.now() ? stored.pending :
        parsePending(JSON.stringify({ ...JSON.parse(await this.core.text({ kind: 'create_link', device, relay: relayUrl(relay) })), expires_at_ms: Date.now() + TTL }));
      if (this.canceled) return;
      if (pending.device !== device) throw new Error('different device');
      this.pending = pending;
      this.expiry = setTimeout(() => this.fail(EXPIRED), Math.max(0, pending.expires_at_ms - Date.now()));
      await this.vault.save({ kind: 'pending', pending });
      if (!this.active()) return;
      const cert = hex(await this.vault.sign(await this.core.bytes({ kind: 'device_cert_message', device })));
      if (!this.active()) return;
      const login = { identity: device, device, cert, relay: pending.relay };
      const state = (connection: ConnectionState) => {
        this.connection = connection;
        if (!this.active()) return;
        if (connection.kind === 'failed') this.fail('Não foi possível conectar este aparelho. Tente de novo.');
        else this.changed({ kind: 'waiting', pending, connection });
      };
      const ready = () => this.fetch();
      this.transport = this.factory ? this.factory(state, ready, login) :
        new AuthenticatedTransport(this.core, this.vault, login, state, async (frame) => { if (frame.kind === 'ready') await ready(); });
      this.changed({ kind: 'waiting', pending, connection: this.connection });
      this.transport.start();
    } catch { if (!this.canceled) this.fail('Não foi possível preparar a conexão. Tente de novo.'); }
  }

  private active(): boolean {
    if (this.canceled) return false;
    if (this.pending && Date.now() >= this.pending.expires_at_ms) { this.fail(EXPIRED); return false; }
    return true;
  }

  private async fetch(): Promise<void> {
    if (!this.active() || !this.pending || !this.transport || this.fetching) return;
    clearTimeout(this.poll);
    this.fetching = true;
    try {
      let response: unknown;
      try { response = await this.transport.request('fetch_link', this.pending.box_id); }
      catch {
        // A Ready callback must not reject into the transport's fatal handler.
        // Connection loss and request timeout both retry, including without a new Ready.
        if (this.active()) this.schedule();
        return;
      }
      if (!this.active()) return;
      const result = object(response);
      if (string(result.kind) !== 'link') throw new Error('unexpected response');
      if (result.sealed === null) { this.schedule(); return; }
      const account = parseAccount(await this.core.text({ kind: 'complete_link', pending: JSON.stringify(this.pending), sealed: bytes(result.sealed) }));
      if (!this.active()) return;
      // Persist before exposing enrollment. No late response can begin a save after expiry/cancellation.
      await this.vault.save({ kind: 'linked', account, snapshot: '' });
      if (!this.active()) return;
      this.stop();
      this.changed({ kind: 'linked', account });
    } catch { if (!this.canceled) this.fail('Não foi possível confirmar este aparelho. Tente de novo.'); }
    finally { this.fetching = false; }
  }

  private schedule(): void {
    clearTimeout(this.poll);
    this.poll = setTimeout(() => { void this.fetch(); }, 700);
  }

  private fail(message: string): void {
    if (this.canceled) return;
    this.stop();
    this.changed({ kind: 'failed', message });
  }

  stop(): void {
    this.canceled = true;
    clearTimeout(this.poll);
    clearTimeout(this.expiry);
    this.transport?.stop();
  }
}
