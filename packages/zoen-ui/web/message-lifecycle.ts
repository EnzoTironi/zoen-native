// Whole operations borrowed from the sole Messages owner.
import { array } from './boundary.ts';
import { parseConversation, parseUpdate } from './message-model.ts';
import { type RelayFrame } from './transport.ts';
import { persistBeforeOutgoing } from './persistence.ts';
import type { MessageOperations } from './message-operations.ts';
type MessageLifecyclePort = Pick<
  MessageOperations,
  | 'cursorRepair'
  | 'enqueue'
  | 'core'
  | 'session'
  | 'conversations'
  | 'closed'
  | 'publish'
  | 'transport'
  | 'ephemeral'
  | 'generation'
  | 'connection'
  | 'notice'
  | 'apply'
  | 'resumeDocumentUploads'
  | 'scheduleMaintenance'
  | 'resumeOutgoingHistory'
  | 'scheduleHistory'
  | 'vault'
  | 'maintenanceTimer'
  | 'maintenanceDue'
>;

export function start(this: MessageLifecyclePort): Promise<void> {
  return this.enqueue(async () => {
    const json = await this.core.text({
      kind: 'restore',
      account: JSON.stringify(this.session.account),
      snapshot: this.session.snapshot,
    });
    this.conversations = array(JSON.parse(json)).map(parseConversation);
    this.cursorRepair.configure(await this.core.text({ kind: 'cursor_repair_config' }));
    if (this.closed) return;
    this.publish();
    this.transport.start();
  });
}

export function received(
  this: MessageLifecyclePort,
  frame: RelayFrame,
  raw: Uint8Array,
): Promise<void> {
  if (frame.kind === 'presence' || frame.kind === 'ephemeral') {
    this.ephemeral?.(frame);
    if (frame.kind === 'presence' || (frame.event !== 'read' && frame.event !== 'delivered'))
      return Promise.resolve();
  }
  const generation = this.generation;
  const readyAt = this.cursorRepair.now();
  if (frame.kind === 'ready') this.cursorRepair.start(generation, readyAt);
  return this.enqueue(async () => {
    if (generation !== this.generation || this.connection.kind !== 'ready') return;
    if (frame.kind === 'ready' || frame.kind === 'pong')
      await this.core.text({ kind: 'claim_clock', value: this.transport.claimClock });
    if (!this.cursorRepair.check(generation)) return;
    // Ready owns its outer watchdog from arrival. All later verified ingestion
    // shares the serial Core clock with reconcile, before any durable save.
    const repairNow = frame.kind === 'ready' ? readyAt : this.cursorRepair.now();
    const update =
      frame.kind === 'ready'
        ? await this.core.text({ kind: 'bootstrap', repairNow })
        : await this.core.text({ kind: 'receive', bytes: raw, now: Date.now(), repairNow });
    if (frame.kind === 'rejected' && frame.permanent) this.notice = frame.reason;
    await this.apply(update, generation);
    {
      try {
        await this.resumeDocumentUploads(generation);
      } catch (error) {
        if (!(error as { documentUploadRetry?: boolean }).documentUploadRetry) throw error;
        this.notice =
          'Uma página aguarda conexão para terminar de enviar. O rascunho foi preservado.';
        this.scheduleMaintenance(5000, generation, true);
        this.publish();
      }
      await this.resumeOutgoingHistory(generation);
      if (frame.kind === 'ready') this.scheduleHistory();
    }
    if (frame.kind === 'sync_done') this.scheduleHistory();
  });
}

export async function apply(
  this: MessageLifecyclePort,
  json: string,
  generation: number,
): Promise<void> {
  const update = parseUpdate(json);
  await persistBeforeOutgoing({
    snapshot: { ...this.session, snapshot: update.snapshot },
    save: (snapshot) => this.vault.save(snapshot),
    frames: update.frames,
    emit: (frame) => {
      // A reconnect bootstraps again from the saved outbox; never emit stale frames.
      if (this.closed || this.connection.kind !== 'ready' || generation !== this.generation) return;
      if (!this.cursorRepair.check(generation)) return;
      this.transport.sendRetained(frame);
    },
  });
  if (this.closed || generation !== this.generation) return;
  this.cursorRepair.update(update.cursorRepair, generation);
  if (this.closed || generation !== this.generation) return;
  this.conversations = update.conversations;
  this.scheduleMaintenance(update.maintenance, generation);
  if (update.historyReady) this.scheduleHistory();
  this.publish();
}

export function scheduleMaintenance(
  this: MessageLifecyclePort,
  delay: number | null,
  generation: number,
  onlyEarlier: boolean,
): void {
  const due = delay === null ? 0 : Date.now() + delay;
  if (onlyEarlier && this.maintenanceTimer && this.maintenanceDue <= due) return;
  clearTimeout(this.maintenanceTimer);
  this.maintenanceDue = due;
  if (
    delay === null ||
    this.closed ||
    this.connection.kind !== 'ready' ||
    generation !== this.generation
  )
    return;
  if (delay < 1 || delay > 60_000) throw new Error('Intervalo inválido do núcleo.');
  this.maintenanceTimer = setTimeout(() => {
    this.maintenanceTimer = undefined;
    this.maintenanceDue = 0;
    void this.enqueue(async () => {
      if (this.closed || generation !== this.generation || this.connection.kind !== 'ready') return;
      await this.apply(await this.core.text({ kind: 'maintenance', now: Date.now() }), generation);
      await this.resumeOutgoingHistory(generation);
      try {
        await this.resumeDocumentUploads(generation);
      } catch (error) {
        if (!(error as { documentUploadRetry?: boolean }).documentUploadRetry) throw error;
        this.notice = 'A conversa aguarda conexão para terminar de enviar.';
        this.scheduleMaintenance(5000, generation, true);
        this.publish();
      }
    }).catch(() => {});
  }, delay);
}
