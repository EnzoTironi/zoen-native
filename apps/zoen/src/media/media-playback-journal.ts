import type { MediaPlaybackScope } from '../core/generated/MediaPlaybackScope';
import type { MediaPlaybackPosition } from '../core/generated/MediaPlaybackPosition';
import type { MediaPlaybackBookmark } from '../core/generated/MediaPlaybackBookmark';
import { sourceKey, type MessageAudioSource, type Metadata } from './message-audio-contract.ts';
type Port = {
  current(source: MessageAudioSource): boolean;
  read(scope: MediaPlaybackScope): Promise<MediaPlaybackBookmark>;
  save(
    scope: MediaPlaybackScope,
    revision: string,
    position: MediaPlaybackPosition,
  ): Promise<MediaPlaybackBookmark>;
};
type Slot = {
  scope: MediaPlaybackScope;
  revision: string;
  last: MediaPlaybackPosition | null;
  pending: MediaPlaybackPosition | null;
  task: Promise<void> | null;
  failed: boolean;
};
/** A transient writer, not another database. Rust owns encryption, CAS and bounded retention. */
export class MediaPlaybackJournal {
  private readonly slots = new Map<string, Slot>();
  private readonly port: Port;
  private serial = 0;
  private readonly reads = new Map<string, number>();
  constructor(port: Port) {
    this.port = port;
  }
  async load(source: MessageAudioSource, version: number): Promise<Metadata | null> {
    if (!this.port.current(source)) return null;
    const key = sourceKey(source),
      ticket = ++this.serial;
    this.reads.set(key, ticket);
    const current = () => this.port.current(source) && this.reads.get(key) === ticket;
    const scope: MediaPlaybackScope = {
      expected_identity_id: source.scope.identityId,
      expected_device_id: source.scope.deviceId,
      space_id: source.spaceId,
      message_id: source.messageId,
      item_id: source.itemId,
      version,
      sha256: source.sha256,
    };
    try {
      await this.slots.get(key)?.task;
      if (!current()) return null;
      const value = await this.port.read(scope);
      if (!current()) return null;
      this.slots.set(sourceKey(source), {
        scope,
        revision: value.revision,
        last: value.position,
        pending: null,
        task: null,
        failed: false,
      });
      while (this.slots.size > 128) {
        const stale = [...this.slots.entries()].find(([, slot]) => !slot.task);
        if (!stale) break;
        this.slots.delete(stale[0]);
      }
      const position = value.position;
      return position
        ? {
            seconds: position.position_ms >= position.duration_ms ? 0 : position.position_ms / 1000,
            duration: position.duration_ms / 1000,
            rate: position.rate_milli === 1500 ? 1.5 : position.rate_milli === 2000 ? 2 : 1,
          }
        : null;
    } catch {
      return null;
    } finally {
      if (this.reads.get(key) === ticket) this.reads.delete(key);
    } // A checkpoint failure never authorizes media or prevents ordinary playback.
  }
  remember(source: MessageAudioSource, value: Metadata, force: boolean) {
    const slot = this.slots.get(sourceKey(source));
    if (
      !slot ||
      slot.failed ||
      !this.port.current(source) ||
      !Number.isFinite(value.seconds) ||
      !Number.isFinite(value.duration) ||
      value.duration <= 0
    )
      return;
    const duration_ms = Math.round(value.duration * 1000);
    if (duration_ms > 604_800_000) return;
    const next = {
      position_ms: Math.max(0, Math.min(duration_ms, Math.round(value.seconds * 1000))),
      duration_ms,
      rate_milli: value.rate * 1000,
    };
    const last = slot.pending ?? slot.last;
    if (
      last &&
      last.position_ms === next.position_ms &&
      last.duration_ms === next.duration_ms &&
      last.rate_milli === next.rate_milli
    )
      return;
    if (
      !force &&
      last &&
      last.rate_milli === next.rate_milli &&
      Math.floor(last.position_ms / 5000) === Math.floor(next.position_ms / 5000)
    )
      return;
    slot.pending = next;
    if (slot.task) return;
    const task = (async () => {
      while (slot.pending && this.port.current(source) && !slot.failed) {
        const position = slot.pending;
        slot.pending = null;
        try {
          const result = await this.port.save(slot.scope, slot.revision, position);
          slot.revision = result.revision;
          slot.last = result.position;
        } catch {
          slot.failed = true;
          slot.pending = null;
        } // Never replay a stale/uncertain writer over another player's checkpoint.
      }
    })();
    slot.task = task;
    void task.finally(() => {
      if (slot.task === task) slot.task = null;
    });
  }
  async flush() {
    await Promise.all([...this.slots.values()].map((slot) => slot.task));
  }
}
