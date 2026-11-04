import type { MediaPlaybackScope } from '../core/generated/MediaPlaybackScope';
import type { MediaPlaybackBookmark } from '../core/generated/MediaPlaybackBookmark';
import type { MediaPlaybackPosition } from '../core/generated/MediaPlaybackPosition';
import { MediaPlaybackJournal } from './media-playback-journal.ts';

export type CheckpointVideoPlayer = {
  currentTime: number;
  readonly duration: number;
  readonly playing: boolean;
  pause(): void;
};
export class VideoCheckpoint {
  private readonly journal: MediaPlaybackJournal;
  private readonly source;
  private live = true;
  private loaded = false;
  private pending: number | null = null;
  private ready = false;
  constructor(
    privateTarget: { scope: MediaPlaybackScope; epoch: number },
    privatePort: {
      player: CheckpointVideoPlayer;
      current(): boolean;
      read(scope: MediaPlaybackScope): Promise<MediaPlaybackBookmark>;
      save(
        scope: MediaPlaybackScope,
        revision: string,
        position: MediaPlaybackPosition,
      ): Promise<MediaPlaybackBookmark>;
      ready(): void;
    },
  ) {
    this.port = privatePort;
    const scope = privateTarget.scope;
    this.source = {
      scope: {
        identityId: scope.expected_identity_id,
        deviceId: scope.expected_device_id,
        epoch: privateTarget.epoch,
      },
      spaceId: scope.space_id,
      messageId: scope.message_id,
      itemId: scope.item_id,
      sha256: scope.sha256,
      durationMs: 0,
    };
    this.version = scope.version;
    this.journal = new MediaPlaybackJournal({
      current: () => this.live && this.port.current(),
      read: privatePort.read,
      save: privatePort.save,
    });
  }
  private readonly version: number;
  private readonly port;
  open() {
    return this.load(false);
  }
  reload() {
    return this.load(true);
  }
  private async load(reloading: boolean) {
    if (!this.live || !this.port.current()) return;
    const origin = this.port.player.currentTime;
    const value = await this.journal.load(this.source, this.version);
    if (!this.live || !this.port.current()) return;
    if (reloading && (this.port.player.playing || this.port.player.currentTime !== origin)) return;
    this.loaded = true;
    this.ready = false;
    this.pending = value?.seconds ?? null;
    this.update();
  }
  update(force = false) {
    if (!this.live || !this.port.current()) {
      if (this.port.player.playing) this.port.player.pause();
      return;
    }
    if (!this.loaded) return;
    const player = this.port.player;
    if (!Number.isFinite(player.duration) || player.duration <= 0) return;
    if (!this.ready) {
      if (this.pending !== null)
        player.currentTime = Math.min(player.duration, Math.max(0, this.pending));
      this.pending = null;
      this.ready = true;
      this.port.ready();
    }
    this.journal.remember(
      this.source,
      { seconds: player.currentTime, duration: player.duration, rate: 1 },
      force || !player.playing,
    );
  }
  pause() {
    if (!this.live) return;
    if (this.port.player.playing) this.port.player.pause();
    this.update(true);
  }
  retire() {
    if (!this.live) return;
    this.pause();
    this.live = false;
  }
  flush() {
    return this.journal.flush();
  }
}

/** Hook-owned transport adapter. Its observable state follows the real native session. */
export class VideoCheckpointBinding {
  private session: VideoCheckpoint | null = null;
  private readonly player: CheckpointVideoPlayer;
  private state: 'loading' | 'ready' | 'unavailable';
  private readonly listeners = new Set<() => void>();
  constructor(player: CheckpointVideoPlayer, scope: string) {
    this.player = player;
    this.state = scope === 'null' ? 'ready' : 'loading';
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  publish(state: 'ready' | 'unavailable') {
    if (state === this.state) return;
    this.state = state;
    for (const listener of this.listeners) listener();
  }
  attach(session: VideoCheckpoint) {
    this.session = session;
  }
  readonly checkpoint = {
    flush: () => this.flush(),
    reload: () => this.session?.reload() ?? Promise.resolve(),
  };
  flush = async () => {
    if (this.session) {
      this.session.pause();
      await this.session.flush();
    } else if (this.player.playing) this.player.pause();
  };
}
