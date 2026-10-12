import { bytes, hex, message, object, string } from './boundary.ts';
import { parseAccount, parsePending, relayUrl, type LinkedAccount, type PendingLink, type StoredSession } from './session.ts';
import { AuthenticatedTransport, type ConnectionState } from './transport.ts';
import { CoreWorker } from './worker-client.ts';
import { BrowserVault } from './vault.ts';

export type PairingState =
  | { kind: 'starting' }
  | { kind: 'waiting'; pending: PendingLink; connection: ConnectionState }
  | { kind: 'linked'; account: LinkedAccount }
  | { kind: 'failed'; message: string };

export class Pairing {
  private transport: AuthenticatedTransport | undefined;
  private poll: ReturnType<typeof setTimeout> | undefined;
  private canceled = false;
  private pending: PendingLink | undefined;
  private connection: ConnectionState = { kind: 'connecting' };

  constructor(private readonly vault: BrowserVault, private readonly core: CoreWorker,
    private readonly changed: (state: PairingState) => void) {}

  async start(relay: string, stored: StoredSession): Promise<void> {
    this.changed({ kind: 'starting' });
    try {
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
    // Enrollment becomes visible only after the encrypted account is durably saved.
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
