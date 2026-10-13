import { bytes, hex, message, object, string } from './boundary.ts';
import { parseAccount, parsePending, relayUrl, type LinkedAccount, type PendingLink, type StoredSession } from './session.ts';
import { AuthenticatedTransport, type ConnectionState, type Login } from './transport.ts';
import type { CoreWorker } from './worker-client.ts';
import type { BrowserVault } from './vault.ts';

type PairingVault = Pick<BrowserVault, 'device' | 'load' | 'save' | 'sign'>;
type PairingCore = Pick<CoreWorker, 'bytes' | 'text'>;

export type PairingState =
  | { kind: 'starting' }
  | { kind: 'waiting'; pending: PendingLink; connection: ConnectionState }
  | { kind: 'confirming'; pending: PendingLink; account: LinkedAccount; message: string | null }
  | { kind: 'linked'; account: LinkedAccount }
  | { kind: 'failed'; message: string };

export class Pairing {
  private transport: AuthenticatedTransport | undefined;
  private poll: ReturnType<typeof setTimeout> | undefined;
  private receiptTimer: ReturnType<typeof setTimeout> | undefined;
  private receiptAttempt = 0;
  private acking = false;
  private canceled = false;
  private pending: PendingLink | undefined;
  private confirming: LinkedAccount | undefined;
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
    try {
      const resumed = await this.confirmingSession(stored);
      if (resumed) {
        await this.confirm(resumed.pending, resumed.account);
        return;
      }
      this.changed({ kind: 'starting' });
      const device = await this.vault.device();
      const pending = stored.kind === 'pending' ? stored.pending :
        parsePending(await this.core.text({ kind: 'create_link', device, relay: relayUrl(relay) }));
      if (pending.device !== device) throw new Error('The pending pairing belongs to a different browser key.');
      if (!await this.persist({ kind: 'pending', pending })) {
        const again = await this.confirmingSession(stored);
        if (again) await this.confirm(again.pending, again.account);
        return;
      }
      if (this.canceled) return;
      this.pending = pending;
      const cert = hex(await this.vault.sign(await this.core.bytes({ kind: 'device_cert_message', device })));
      this.connect({ identity: device, device, cert, relay: pending.relay });
      this.changed({ kind: 'waiting', pending, connection: this.connection });
    } catch (error) {
      if (this.canceled) return;
      if (this.confirming && this.pending) this.noteReceiptFailure(message(error));
      else this.changed({ kind: 'failed', message: message(error) });
    }
  }

  /** Asks again for the receipt. The saved account stays `confirming`. */
  retry(): void {
    if (this.canceled || !this.confirming) return;
    clearTimeout(this.receiptTimer);
    this.receiptAttempt = 0;
    void this.retryReceipt();
  }

  private async confirmingSession(stored: StoredSession): Promise<{ pending: PendingLink; account: LinkedAccount } | undefined> {
    const durable = await this.vault.load();
    if (durable.kind === 'pending' && durable.confirming) return { pending: durable.pending, account: durable.confirming };
    if (stored.kind === 'pending' && stored.confirming) return { pending: stored.pending, account: stored.confirming };
    return undefined;
  }

  /** A bare pending session cannot replace an account already saved as `confirming`. */
  private async persist(session: StoredSession): Promise<boolean> {
    if (session.kind === 'pending' && session.confirming === undefined) {
      const durable = await this.vault.load();
      if (durable.kind === 'pending' && durable.confirming !== undefined) return false;
      if (this.confirming) {
        await this.vault.save({ kind: 'pending', pending: session.pending, confirming: this.confirming });
        return true;
      }
    }
    await this.vault.save(session);
    return true;
  }

  private async fetch(): Promise<void> {
    if (!this.pending || this.canceled || !this.transport || this.confirming) return;
    const result = object(await this.transport.request('fetch_link', this.pending.box_id));
    if (string(result.kind) !== 'link') throw new Error('The relay answered a different pairing request.');
    if (this.canceled) return;
    if (result.sealed === null) {
      this.poll = setTimeout(() => { void this.fetch().catch((error: unknown) => {
        if (!this.canceled && !this.confirming && this.connection.kind === 'ready') this.changed({ kind: 'failed', message: message(error) });
      }); }, 700);
      return;
    }
    const account = parseAccount(await this.core.text({ kind: 'complete_link', pending: JSON.stringify(this.pending), sealed: bytes(result.sealed) }));
    // The account is durable before the receipt. A reload sees `confirming` and sends it again.
    await this.persist({ kind: 'pending', pending: this.pending, confirming: account });
    this.confirming = account;
    if (this.canceled) return;
    this.changed({ kind: 'confirming', pending: this.pending, account, message: null });
    await this.ack(account);
  }

  private async confirm(pending: PendingLink, account: LinkedAccount): Promise<void> {
    if (this.canceled) return;
    const device = await this.vault.device();
    if (pending.device !== device || account.device !== device) {
      throw new Error('The pending pairing belongs to a different browser key.');
    }
    this.pending = pending;
    this.confirming = account;
    this.changed({ kind: 'confirming', pending, account, message: null });
    const cert = hex(await this.vault.sign(await this.core.bytes({ kind: 'device_cert_message', device })));
    if (this.canceled) return;
    this.connect({ identity: device, device, cert, relay: pending.relay });
  }

  private connect(login: Login): void {
    this.transport = new AuthenticatedTransport(this.core, this.vault, login, (connection) => {
      this.onConnection(connection);
    }, async (frame) => {
      if (frame.kind !== 'ready') return;
      try {
        if (this.confirming) await this.ack(this.confirming);
        else await this.fetch();
      } catch (error) {
        if (this.confirming) {
          if (!this.canceled) this.noteReceiptFailure(message(error));
          return;
        }
        throw error;
      }
    });
    this.transport.start();
  }

  private onConnection(connection: ConnectionState): void {
    this.connection = connection;
    if (this.canceled) return;
    if (this.confirming && this.pending) {
      if (connection.kind === 'failed') this.noteReceiptFailure(connection.message);
      return;
    }
    if (connection.kind === 'failed') this.changed({ kind: 'failed', message: connection.message });
    else if (this.pending) this.changed({ kind: 'waiting', pending: this.pending, connection });
  }

  private async ack(account: LinkedAccount): Promise<void> {
    if (!this.pending || this.canceled || !this.transport || this.acking) return;
    this.acking = true;
    try {
      const result = object(await this.transport.request('ack_link', this.pending.box_id));
      if (string(result.kind) !== 'done') throw new Error('The relay refused the link receipt.');
      await this.persist({ kind: 'linked', account, snapshot: '' });
      if (this.canceled) return;
      this.stop();
      this.changed({ kind: 'linked', account });
    } catch (error) {
      if (!this.canceled) this.noteReceiptFailure(message(error));
    } finally {
      this.acking = false;
    }
  }

  private noteReceiptFailure(text: string): void {
    if (this.canceled || !this.confirming || !this.pending) {
      if (!this.canceled) this.changed({ kind: 'failed', message: text });
      return;
    }
    this.changed({ kind: 'confirming', pending: this.pending, account: this.confirming, message: text });
    this.scheduleRetry();
  }

  private scheduleRetry(): void {
    if (this.canceled || !this.confirming) return;
    clearTimeout(this.receiptTimer);
    this.receiptAttempt += 1;
    const delay = Math.min(30_000, 700 * 2 ** Math.min(this.receiptAttempt - 1, 6));
    this.receiptTimer = setTimeout(() => { void this.retryReceipt(); }, delay);
  }

  private async retryReceipt(): Promise<void> {
    if (this.canceled || !this.confirming || !this.pending) return;
    if (this.connection.kind === 'failed' || this.connection.kind === 'stopped' || !this.transport) {
      try { await this.confirm(this.pending, this.confirming); }
      catch (error) { if (!this.canceled) this.noteReceiptFailure(message(error)); }
      return;
    }
    if (this.connection.kind !== 'ready') return;
    await this.ack(this.confirming);
  }

  stop(): void {
    this.canceled = true;
    clearTimeout(this.poll);
    clearTimeout(this.receiptTimer);
    this.transport?.stop();
  }
}
