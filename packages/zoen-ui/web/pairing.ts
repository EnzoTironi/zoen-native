import { bytes, hex, message, object, string } from './boundary.ts';
import { parseAccount, parsePending, relayUrl, type LinkedAccount, type PendingLink, type StoredSession } from './session.ts';
import { AuthenticatedTransport, type ConnectionState } from './transport.ts';
import type { CoreWorker } from './worker-client.ts';
import type { BrowserVault } from './vault.ts';

type PairingVault = Pick<BrowserVault, 'device' | 'save' | 'sign'>;
type PairingCore = Pick<CoreWorker, 'bytes' | 'text'>;

export type PairingState =
  | { kind: 'starting' }
  | { kind: 'waiting'; pending: PendingLink; connection: ConnectionState }
  | { kind: 'confirming' }
  | { kind: 'linked'; account: LinkedAccount }
  | { kind: 'failed'; message: string };

export class Pairing {
  private transport: AuthenticatedTransport | undefined;
  private poll: ReturnType<typeof setTimeout> | undefined;
  private canceled = false;
  private pending: PendingLink | undefined;
  private connection: ConnectionState = { kind: 'connecting' };
  private readonly vault: PairingVault;
  private readonly core: PairingCore;
  private readonly changed: (state: PairingState) => void;

  constructor(vault: PairingVault, core: PairingCore, changed: (state: PairingState) => void) {
    this.vault = vault;
    this.core = core;
    this.changed = changed;
  }

  async start(relay: string, stored: StoredSession): Promise<void> {
    this.changed({ kind: 'starting' });
    try {
      if (stored.kind === 'pending' && stored.confirming) {
        await this.confirm(stored.pending, stored.confirming);
        return;
      }
      const device = await this.vault.device();
      const pending = stored.kind === 'pending' ? stored.pending :
        parsePending(await this.core.text({ kind: 'create_link', device, relay: relayUrl(relay) }));
      if (pending.device !== device) throw new Error('The pending pairing belongs to a different browser key.');
      await this.vault.save({ kind: 'pending', pending });
      if (this.canceled) return;
      this.pending = pending;
      const cert = hex(await this.vault.sign(await this.core.bytes({ kind: 'device_cert_message', device })));
      this.transport = new AuthenticatedTransport(this.core, this.vault,
        { identity: device, device, cert, relay: pending.relay },
        (connection) => {
          this.connection = connection;
          if (this.canceled) return;
          if (connection.kind === 'failed') this.changed({ kind: 'failed', message: connection.message });
          else if (this.pending) this.changed({ kind: 'waiting', pending: this.pending, connection });
        },
        async (frame) => { if (frame.kind === 'ready') await this.fetch(); });
      this.changed({ kind: 'waiting', pending, connection: this.connection });
      this.transport.start();
    } catch (error) { if (!this.canceled) this.changed({ kind: 'failed', message: message(error) }); }
  }

  private async fetch(): Promise<void> {
    if (!this.pending || this.canceled || !this.transport) return;
    const result = object(await this.transport.request('fetch_link', this.pending.box_id));
    if (string(result.kind) !== 'link') throw new Error('The relay answered a different pairing request.');
    if (this.canceled) return;
    if (result.sealed === null) {
      this.poll = setTimeout(() => { void this.fetch().catch((error: unknown) => {
        if (!this.canceled && this.connection.kind === 'ready') this.changed({ kind: 'failed', message: message(error) });
      }); }, 700);
      return;
    }
    const account = parseAccount(await this.core.text({ kind: 'complete_link', pending: JSON.stringify(this.pending), sealed: bytes(result.sealed) }));
    // The account is durable before the receipt. A reload sees `confirming` and sends it again.
    await this.vault.save({ kind: 'pending', pending: this.pending, confirming: account });
    if (this.canceled) return;
    await this.ack(account);
  }

  private async confirm(pending: PendingLink, account: LinkedAccount): Promise<void> {
    if (this.canceled) return;
    const device = await this.vault.device();
    if (pending.device !== device || account.device !== device) {
      throw new Error('The pending pairing belongs to a different browser key.');
    }
    this.pending = pending;
    const cert = hex(await this.vault.sign(await this.core.bytes({ kind: 'device_cert_message', device })));
    this.changed({ kind: 'confirming' });
    this.connect({ identity: device, device, cert, relay: pending.relay }, async () => { await this.ack(account); });
  }

  private connect(login: { identity: string; device: string; cert: string; relay: string },
    onReady: () => Promise<void>): void {
    this.transport = new AuthenticatedTransport(this.core, this.vault, login, (connection) => {
      this.connection = connection;
      if (this.canceled) return;
      if (connection.kind === 'failed') this.changed({ kind: 'failed', message: connection.message });
    }, async (frame) => { if (frame.kind === 'ready') await onReady(); });
    this.transport.start();
  }

  private async ack(account: LinkedAccount): Promise<void> {
    if (!this.pending || this.canceled || !this.transport) return;
    const result = object(await this.transport.request('ack_link', this.pending.box_id));
    if (string(result.kind) !== 'done') throw new Error('The relay refused the link receipt.');
    await this.vault.save({ kind: 'linked', account, snapshot: '' });
    if (this.canceled) return;
    this.stop();
    this.changed({ kind: 'linked', account });
  }

  stop(): void {
    this.canceled = true;
    clearTimeout(this.poll);
    this.transport?.stop();
  }
}
