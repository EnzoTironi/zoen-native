/** One mounted player's revocable playback intent; owns no files or native lifetime. */
export type PlaybackIntent = Readonly<{ owner: PlaybackOwner }>;
export class PlaybackCoordinator {
  private owners = new Set<PlaybackOwner>();
  private current: PlaybackIntent | null = null;

  register(pause: () => void, playing: () => boolean): PlaybackOwner {
    const owner = new PlaybackOwner(this, pause, playing);
    this.owners.add(owner);
    return owner;
  }
  claim(owner: PlaybackOwner): PlaybackIntent | null {
    if (!this.owners.has(owner)) return null;
    const intent = Object.freeze({ owner });
    this.current = intent;
    // Native controls may already be playing before their event reaches JS.
    // Stop those players too, rather than considering only the previous JS claim.
    for (const other of this.owners) if (other !== owner) other.interrupt();
    return this.current === intent ? intent : null;
  }
  isCurrent(intent: PlaybackIntent): boolean {
    return this.current === intent;
  }
  release(intent: PlaybackIntent): void {
    if (this.current === intent) this.current = null;
  }
  remove(owner: PlaybackOwner): void {
    this.owners.delete(owner);
    if (this.current?.owner === owner) this.current = null;
  }
  pauseAll(): void {
    this.current = null;
    for (const owner of this.owners) owner.interrupt();
  }
}

export class PlaybackOwner {
  private closed = false;
  private blocked = false;
  private intent: PlaybackIntent | null = null;
  private coordinator: PlaybackCoordinator;
  private pause: () => void;
  private playing: () => boolean;
  constructor(coordinator: PlaybackCoordinator, pause: () => void, playing: () => boolean) {
    this.coordinator = coordinator;
    this.pause = pause;
    this.playing = playing;
  }
  begin(): PlaybackIntent | null {
    if (this.closed) return null;
    this.blocked = false;
    return (this.intent = this.coordinator.claim(this));
  }
  isCurrent(intent: PlaybackIntent | null): boolean {
    return !this.closed && intent !== null && this.coordinator.isCurrent(intent);
  }
  release(intent: PlaybackIntent | null): void {
    if (!intent) return;
    this.coordinator.release(intent);
    if (this.intent === intent) this.intent = null;
  }
  private actualPlaying(): boolean {
    try {
      return this.playing();
    } catch {
      // A SharedObject can have been released before React's passive cleanup.
      return false;
    }
  }
  refresh(): boolean {
    const actual = this.actualPlaying();
    if (!actual) {
      this.blocked = false;
      this.release(this.intent);
    }
    return actual;
  }
  observe(announcedPlaying: boolean): void {
    if (this.closed) return;
    // Native events are ordered per player. Its stopped acknowledgement settles
    // interruption even if the user has already restarted it before JS catches up.
    // Only actual stopped state may release this owner's current claim.
    if (!announcedPlaying) {
      this.blocked = false;
      this.refresh();
      return;
    }
    if (!this.refresh()) return;
    if (this.blocked) {
      this.interrupt();
      return;
    }
    if (!this.isCurrent(this.intent)) this.begin();
  }
  /** Audio has explicit JS Play intents: SDK auto-resume must never acquire one. */
  observeExplicitPlayback(): void {
    if (!this.closed && this.actualPlaying() && !this.isCurrent(this.intent)) this.interrupt();
  }
  interrupt(): void {
    this.release(this.intent);
    // Already-stopped players need no acknowledgement to become usable again.
    this.blocked = this.actualPlaying();
    // Pause also cancels a native play waiting for data, whose playing flag is false.
    try {
      this.pause();
    } catch {
      /* Native lifetime remains with its hook, including an already released player. */
    }
    // Some native players update synchronously; don't wait for an optional event.
    if (!this.actualPlaying()) this.blocked = false;
  }
  dispose(): void {
    if (this.closed) return;
    this.closed = true;
    this.coordinator.remove(this);
    this.interrupt();
  }
}

export const mediaPlayback = new PlaybackCoordinator();
