import {
  mediaPlayback,
  type PlaybackCoordinator,
  type PlaybackIntent,
  type PlaybackOwner,
} from './playback-owner.ts';

export type AudioClipPlaybackState = Readonly<{
  loaded: boolean;
  playing: boolean;
  buffering: boolean;
  currentSeconds: number;
  durationSeconds: number;
  rate: number;
  error: Error | null;
  loadFailed: boolean;
  busy: boolean;
}>;
export interface AudioClipPlayback {
  subscribe(listener: () => void): () => void;
  getSnapshot(): AudioClipPlaybackState;
  seekSeconds(seconds: number): Promise<void>;
  playFrom(seconds?: number): Promise<void>;
  pause(): void;
  invalidate(): void;
  retry(): void;
  setRate(rate: 1 | 1.5 | 2): void;
}
export type AudioClipStatus = Readonly<{
  isLoaded: boolean;
  playing: boolean;
  isBuffering: boolean;
  currentTime: number;
  duration: number;
  playbackRate: number;
  error?: string | null;
  playbackState?: string;
}>;
/** The hook owns this native lifetime. The controller never releases or changes its source. */
export interface AudioClipPort {
  playing(): boolean;
  currentSeconds(): number;
  play(): void;
  pause(): void;
  seek(seconds: number): Promise<void>;
  setRate(rate: number): void;
  reload(): void;
  preparePlayback(): Promise<void>;
}
type Binding = { port: AudioClipPort; live: boolean; owner: PlaybackOwner };
type Request = {
  generation: number;
  target: number | null;
  intent: PlaybackIntent | null;
  play: boolean;
  settled: boolean;
  resolve(): void;
  reject(error: Error): void;
};
const positive = (value: number) => Number.isFinite(value) && value > 0;
const initial = (durationSeconds = 0): AudioClipPlaybackState =>
  Object.freeze({
    loaded: false,
    playing: false,
    buffering: false,
    currentSeconds: 0,
    durationSeconds,
    rate: 1,
    error: null,
    loadFailed: false,
    busy: false,
  });
const alwaysCurrent = () => true;

/** One immutable source's transport. No files, persistence, UI, timer or second audio engine. */
export class AudioClipController implements AudioClipPlayback {
  private state: AudioClipPlaybackState;
  private readonly listeners = new Set<() => void>();
  private binding: Binding | null = null;
  private generation = 0;
  private enabled = true;
  private isCurrent: () => boolean = alwaysCurrent;
  private durationHint: number;
  private pending: Request | null = null;
  private active: Request | null = null;
  private latest: Request | null = null;
  private working = false;
  private lastSeconds = 0;
  private readonly coordinator: PlaybackCoordinator;
  readonly sourceIdentity: unknown;
  constructor(
    durationMs = 0,
    coordinator: PlaybackCoordinator = mediaPlayback,
    sourceIdentity: unknown = null,
  ) {
    this.coordinator = coordinator;
    this.sourceIdentity = sourceIdentity;
    this.durationHint = positive(durationMs) ? durationMs / 1000 : 0;
    this.state = initial(this.durationHint);
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<AudioClipPlaybackState>) {
    const next = { ...this.state, ...patch };
    if (
      Object.keys(next).every((key) =>
        Object.is(next[key as keyof typeof next], this.state[key as keyof typeof next]),
      )
    )
      return;
    this.state = Object.freeze(next);
    for (const listener of this.listeners) listener();
  }
  connect(port: AudioClipPort): () => void {
    this.disconnect(this.binding);
    const binding: Binding = { port, live: true, owner: null! };
    binding.owner = this.coordinator.register(
      () => {
        if (!binding.live) return;
        if (this.binding === binding) {
          this.cancelRequests();
          this.publish({ playing: false, busy: false, currentSeconds: this.lastSeconds });
        }
        port.pause();
      },
      () => binding.live && port.playing(),
    );
    this.binding = binding;
    return () => this.disconnect(binding);
  }
  private disconnect(binding: Binding | null) {
    if (!binding || this.binding !== binding) return;
    this.binding = null;
    this.cancelRequests();
    // Layout cleanup pauses while the hook still owns a live SharedObject.
    binding.owner.dispose();
    binding.live = false;
    this.lastSeconds = 0;
    this.publish(initial(this.durationHint));
  }
  configure(enabled: boolean, isCurrent = alwaysCurrent, durationMs = 0) {
    this.enabled = enabled;
    this.isCurrent = isCurrent;
    this.durationHint = positive(durationMs) ? durationMs / 1000 : 0;
    if (!this.admitted(this.binding)) this.invalidate();
  }
  private admitted(binding: Binding | null): binding is Binding {
    if (!binding || this.binding !== binding || !binding.live || !this.enabled) return false;
    try {
      return this.isCurrent();
    } catch {
      return false;
    }
  }
  private current(binding: Binding, request: Request) {
    return (
      this.admitted(binding) &&
      request.generation === this.generation &&
      this.latest === request &&
      (!request.play || binding.owner.isCurrent(request.intent))
    );
  }
  private finish(request: Request, error?: Error) {
    if (request.settled) return;
    request.settled = true;
    if (error) request.reject(error);
    else request.resolve();
  }
  private cancelRequests() {
    ++this.generation;
    if (this.pending) this.finish(this.pending);
    if (this.active) this.finish(this.active);
    this.pending = null;
    this.latest = null;
  }
  invalidate = () => {
    this.cancelRequests();
    this.binding?.owner.interrupt();
    this.publish({ playing: false, busy: false, currentSeconds: this.lastSeconds });
  };
  pause = () => this.invalidate();
  /** Called by the native status listener too, so SDK auto-resume cannot acquire an intent. */
  observe(status: AudioClipStatus) {
    const binding = this.binding;
    if (!binding?.live) return;
    if (!this.admitted(binding)) this.invalidate();
    binding.owner.observeExplicitPlayback();
    const durationSeconds = positive(status.duration) ? status.duration : this.durationHint;
    this.lastSeconds = Number.isFinite(status.currentTime)
      ? Math.max(0, Math.min(durationSeconds, status.currentTime))
      : 0;
    const loadFailed = Boolean(status.error) || status.playbackState === 'failed';
    const message = status.error || 'Audio could not be loaded';
    const error = loadFailed
      ? this.state.loadFailed && this.state.error?.message === message
        ? this.state.error
        : new Error(message)
      : this.state.loadFailed
        ? null
        : this.state.error;
    let playing = false;
    try {
      playing = this.admitted(binding) && binding.port.playing();
    } catch {
      /* released by its hook */
    }
    this.publish({
      loaded: status.isLoaded && !loadFailed,
      playing,
      buffering: status.isBuffering,
      durationSeconds,
      currentSeconds: this.latest?.target ?? this.lastSeconds,
      rate: positive(status.playbackRate) ? status.playbackRate : 1,
      loadFailed,
      error,
    });
  }
  private submit(play: boolean, seconds?: number): Promise<void> {
    const binding = this.binding;
    if (
      !this.admitted(binding) ||
      !this.state.loaded ||
      (seconds !== undefined && !Number.isFinite(seconds))
    )
      return Promise.resolve();
    const target =
      seconds === undefined ? null : Math.max(0, Math.min(this.state.durationSeconds, seconds));
    const intent = play ? binding.owner.begin() : null;
    if (play && !intent) return Promise.resolve();
    // A scrub can cancel a pending Play without stopping already audible playback.
    if (!play && this.latest?.play) {
      let playing = false;
      try {
        playing = binding.port.playing();
      } catch {
        /* no live playback */
      }
      if (!playing) binding.owner.interrupt();
    }
    if (this.pending) this.finish(this.pending);
    if (this.active) this.finish(this.active);
    const task = new Promise<void>((resolve, reject) => {
      this.pending = this.latest = {
        generation: this.generation,
        target,
        play,
        intent,
        settled: false,
        resolve,
        reject,
      };
    });
    this.publish({ busy: true, error: null, currentSeconds: target ?? this.state.currentSeconds });
    this.work();
    return task;
  }
  private work() {
    if (this.working) return;
    this.working = true;
    void Promise.resolve().then(async () => {
      try {
        while (this.pending) {
          const request = this.pending;
          this.pending = null;
          this.active = request;
          const binding = this.binding;
          try {
            if (!binding || !this.current(binding, request)) continue;
            if (request.play) {
              await binding.port.preparePlayback();
              if (!this.current(binding, request)) continue;
            }
            let target = request.target;
            if (
              request.play &&
              target === null &&
              this.state.durationSeconds > 0 &&
              binding.port.currentSeconds() >= this.state.durationSeconds - 0.05
            )
              target = 0;
            if (target !== null) {
              await binding.port.seek(target);
              if (!this.current(binding, request)) continue;
            }
            if (request.play) binding.port.play();
            if (this.current(binding, request)) {
              this.lastSeconds = Math.max(
                0,
                Math.min(this.state.durationSeconds, binding.port.currentSeconds()),
              );
              this.publish({ playing: binding.port.playing(), currentSeconds: this.lastSeconds });
            }
          } catch (cause) {
            if (binding && this.current(binding, request)) {
              const error =
                cause instanceof Error ? cause : new Error('Audio playback failed', { cause });
              this.finish(request, error);
              binding.owner.interrupt();
              this.publish({ error, playing: false });
            }
          } finally {
            this.finish(request);
            if (this.active === request) this.active = null;
            if (this.latest === request) {
              this.latest = null;
              this.publish({ busy: false, currentSeconds: this.lastSeconds });
            }
          }
        }
      } finally {
        this.working = false;
        if (this.pending) this.work();
      }
    });
  }
  seekSeconds = (seconds: number) => this.submit(false, seconds);
  playFrom = (seconds?: number) => this.submit(true, seconds);
  setRate = (rate: 1 | 1.5 | 2) => {
    const binding = this.binding;
    if (!this.admitted(binding) || !this.state.loaded || ![1, 1.5, 2].includes(rate)) return;
    try {
      binding.port.setRate(rate);
      this.publish({ rate, error: null });
    } catch (cause) {
      this.publish({
        error: cause instanceof Error ? cause : new Error('Audio playback failed', { cause }),
      });
    }
  };
  retry = () => {
    const binding = this.binding;
    if (!this.admitted(binding)) return;
    this.invalidate();
    this.publish({ loaded: false, buffering: true, error: null, loadFailed: false });
    try {
      binding.port.reload();
    } catch (cause) {
      this.publish({
        buffering: false,
        loadFailed: true,
        error: cause instanceof Error ? cause : new Error('Audio reload failed', { cause }),
      });
    }
  };
}
