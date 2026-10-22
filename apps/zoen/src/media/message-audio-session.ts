import type { AudioClipPlaybackState } from './audio-clip-controller.ts';
import { MessageAudioChannel } from './message-audio-channel.ts';
import {
  metadata,
  scopeKey,
  snapshot,
  sourceKey,
  type Active,
  type Focus,
  type Intent,
  type MessageAudioPort,
  type MessageAudioRuntime,
  type MessageAudioSource,
  type Metadata,
  type Preparation,
  type Retired,
} from './message-audio-contract.ts';
import type { LocalMediaFile } from './types.ts';
export { MessageAudioChannel } from './message-audio-channel.ts';
export type {
  MessageAudioPort,
  MessageAudioRuntime,
  MessageAudioScope,
  MessageAudioSource,
} from './message-audio-contract.ts';

/** One account-session transport; Rust remains the only durable owner. */
export class MessageAudioSession {
  private readonly port: MessageAudioPort;
  private readonly channels = new Map<string, Set<MessageAudioChannel>>();
  private readonly positions = new Map<string, Metadata>();
  private readonly focuses: Focus[] = [];
  private readonly work = new Set<Promise<unknown>>();
  private readonly retired = new Set<Retired>();
  private active: Active | null = null;
  private preparing: Preparation | null = null;
  private serial = 0;
  private changes = 0;
  private lifecycle = 0;
  private foreground = true;
  private disposed = false;
  private retiring = false;
  private blockedScope: string | null = null;
  private currentScope: string | null = null;
  constructor(port: MessageAudioPort) {
    this.port = port;
  }
  channel(source: MessageAudioSource) {
    return new MessageAudioChannel(this, source);
  }
  private admitted(source: MessageAudioSource) {
    const focus = this.focuses.at(-1);
    return (
      !this.disposed &&
      !this.retiring &&
      this.foreground &&
      scopeKey(source.scope) === scopeKey(this.port.scope()) &&
      focus?.scope === scopeKey(source.scope) &&
      focus.spaceId === source.spaceId &&
      this.port.authorized(source)
    );
  }
  private cached(source: MessageAudioSource) {
    return this.positions.get(sourceKey(source)) ?? metadata(source);
  }
  initial(source: MessageAudioSource) {
    const active = this.active;
    return active?.channel.key === sourceKey(source)
      ? active.channel.getSnapshot()
      : snapshot(this.cached(source), this.admitted(source));
  }
  attach(channel: MessageAudioChannel) {
    const current = this.channels.get(channel.key) ?? new Set();
    current.add(channel);
    this.channels.set(channel.key, current);
    channel.publish(this.initial(channel.source));
  }
  detach(channel: MessageAudioChannel) {
    const current = this.channels.get(channel.key);
    current?.delete(channel);
    if (!current?.size) this.channels.delete(channel.key);
  }
  private publish(channel: MessageAudioChannel, state: AudioClipPlaybackState) {
    channel.publish(state);
    for (const other of this.channels.get(channel.key) ?? [])
      if (other !== channel) other.publish(state);
  }
  private remember(channel: MessageAudioChannel, state = channel.getSnapshot()) {
    if (scopeKey(channel.source.scope) !== scopeKey(this.port.scope())) return;
    const rate = state.rate === 1.5 || state.rate === 2 ? state.rate : 1;
    this.positions.delete(channel.key);
    this.positions.set(channel.key, {
      seconds: state.currentSeconds,
      duration: state.durationSeconds,
      rate,
    });
    while (this.positions.size > 128) {
      const evicted = [...this.positions.keys()].find(
        (key) => key !== this.active?.channel.key && key !== this.preparing?.channel.key,
      );
      if (!evicted) break;
      this.positions.delete(evicted);
    }
  }
  /** A token belongs to one focused route; late blur cannot cancel a newer route. */
  focus(spaceId: string) {
    const token = Object.freeze({ scope: scopeKey(this.port.scope()) ?? '', spaceId });
    this.focuses.push(token);
    this.reconcile();
    return () => {
      const index = this.focuses.indexOf(token);
      if (index >= 0) this.focuses.splice(index, 1);
      this.reconcile();
    };
  }
  setForeground(foreground: boolean) {
    this.foreground = foreground;
    this.reconcile();
  }
  /** Synchronous admission fence; no cleanup waits for React to unmount a row. */
  reconcile() {
    const next = scopeKey(this.port.scope());
    if (next !== this.currentScope) {
      this.currentScope = next;
      this.stop();
      this.positions.clear();
      if (next && next !== this.blockedScope && !this.retired.size && !this.work.size)
        this.retiring = false;
    }
    if (
      (this.active && !this.admitted(this.active.channel.source)) ||
      (this.preparing && !this.admitted(this.preparing.channel.source))
    )
      this.stop();
    for (const group of this.channels.values())
      for (const channel of group) {
        if (this.active?.channel.key !== channel.key && this.preparing?.channel.key !== channel.key)
          channel.publish(snapshot(this.cached(channel.source), this.admitted(channel.source)));
      }
  }
  private track<T>(task: Promise<T>): Promise<T> {
    this.work.add(task);
    void task.finally(() => this.work.delete(task)).catch(() => {});
    return task;
  }
  private intent(cancel: () => void): Intent {
    const value: Intent = { owner: null!, token: null, handoff: false, cancelled: false };
    value.owner = this.port.coordinator.register(
      () => {
        if (value.handoff || value.cancelled) return;
        value.cancelled = true;
        cancel();
      },
      () => false,
    );
    value.token = value.owner.begin();
    return value;
  }
  private owns(intent: Intent) {
    return !intent.cancelled && intent.owner.isCurrent(intent.token);
  }
  private handoff(intent: Intent) {
    intent.handoff = true;
    intent.owner.dispose();
  }
  private cancelIntent(intent: Intent | null) {
    if (!intent) return;
    intent.cancelled = true;
    intent.owner.dispose();
  }
  private retire(file: LocalMediaFile, runtime: MessageAudioRuntime | null) {
    const value: Retired = { file, runtime, runtimeDisposed: !runtime, task: null };
    this.retired.add(value);
    void this.clean(value).catch(() => {});
  }
  private clean(value: Retired): Promise<void> {
    if (value.task) return value.task;
    const task = (async () => {
      if (!value.runtimeDisposed) {
        await value.runtime!.dispose();
        value.runtimeDisposed = true;
      }
      await this.port.release(value.file);
      this.retired.delete(value);
    })();
    value.task = task;
    void task
      .finally(() => {
        if (value.task === task) value.task = null;
      })
      .catch(() => {});
    return task;
  }
  private stop() {
    ++this.serial;
    const preparing = this.preparing;
    this.preparing = null;
    this.cancelIntent(preparing?.intent ?? null);
    if (preparing)
      this.publish(
        preparing.channel,
        snapshot(this.cached(preparing.channel.source), this.admitted(preparing.channel.source)),
      );
    const active = this.active;
    this.active = null;
    if (!active) return;
    if (!active.initial && !active.validating) this.remember(active.channel);
    active.unsubscribe();
    this.cancelIntent(active.firstPlay);
    this.cancelIntent(active.validationIntent);
    active.runtime.playback.invalidate();
    this.publish(
      active.channel,
      snapshot(this.cached(active.channel.source), this.admitted(active.channel.source)),
    );
    this.retire(active.file, active.runtime);
  }
  private current(active: Active) {
    return (
      this.active === active &&
      active.serial === this.serial &&
      !active.validating &&
      this.admitted(active.channel.source)
    );
  }
  private failure(channel: MessageAudioChannel, reason: unknown) {
    if (!this.admitted(channel.source)) return;
    const error =
      reason instanceof Error ? reason : new Error('Audio could not be opened.', { cause: reason });
    this.publish(channel, {
      ...snapshot(this.cached(channel.source)),
      loaded: false,
      loadFailed: true,
      error,
    });
  }
  private async verified(source: MessageAudioSource, current: () => boolean) {
    let version: number;
    let revision: number;
    do {
      revision = this.changes;
      version = await this.port.verify(source);
      if (!current()) return null;
    } while (revision !== this.changes);
    return version;
  }
  play(channel: MessageAudioChannel, seconds?: number): Promise<void> {
    if (!this.admitted(channel.source) || (seconds !== undefined && !Number.isFinite(seconds)))
      return Promise.resolve();
    const existing = this.active;
    if (existing?.channel.key === channel.key && !existing.initial && this.current(existing)) {
      return existing.runtime.playback.playFrom(seconds);
    }
    this.stop();
    const serial = this.serial;
    const preparing: Preparation = { channel, serial, intent: null! };
    this.preparing = preparing;
    preparing.intent = this.intent(() => {
      if (this.preparing === preparing || (this.active?.serial === serial && this.active.initial))
        this.stop();
    });
    const current = () =>
      this.preparing === preparing &&
      this.serial === serial &&
      this.admitted(channel.source) &&
      this.owns(preparing.intent);
    this.publish(channel, { ...snapshot(this.cached(channel.source)), busy: true });
    return this.track(
      (async () => {
        let file: LocalMediaFile | null = null;
        let runtime: MessageAudioRuntime | null = null;
        try {
          await this.drainRetired();
          if (!current()) return;
          const version = await this.verified(channel.source, current);
          if (version === null || !current()) return;
          file = await this.port.materialize(channel.source, version);
          if (!file) throw new Error('Audio is still arriving.');
          if (!current()) return;
          const verifiedVersion = await this.verified(channel.source, current);
          if (!current()) return;
          if (
            verifiedVersion !== version ||
            file.itemId !== channel.source.itemId ||
            file.version !== version ||
            file.info.sha256 !== channel.source.sha256 ||
            !file.info.mime.startsWith('audio/')
          )
            throw new Error('The audio message changed.');
          const active: Active = {
            channel,
            file,
            runtime: null!,
            serial,
            unsubscribe: () => {},
            firstPlay: preparing.intent,
            initial: true,
            startSeconds: seconds ?? this.cached(channel.source).seconds,
            validating: false,
            validationAgain: false,
            validationIntent: null,
          };
          runtime = this.port.create(channel.source, file, () => this.current(active));
          if (runtime.failure) throw runtime.failure;
          active.runtime = runtime;
          this.active = active;
          this.preparing = null;
          file = null;
          runtime = null;
          active.unsubscribe = active.runtime.playback.subscribe(() => this.observe(active));
          this.observe(active);
        } catch (reason) {
          if (current()) this.failure(channel, reason);
        } finally {
          if (file) this.retire(file, runtime);
          if (this.preparing === preparing) {
            this.preparing = null;
            this.cancelIntent(preparing.intent);
          }
        }
      })(),
    );
  }
  private observe(active: Active) {
    if (this.active !== active) return;
    const state = active.runtime.playback.getSnapshot();
    if (active.firstPlay && state.loadFailed) {
      this.cancelIntent(active.firstPlay);
      active.firstPlay = null;
    }
    if (active.firstPlay && state.loaded && this.current(active)) {
      const intent = active.firstPlay;
      active.firstPlay = null;
      if (this.owns(intent)) {
        this.handoff(intent);
        active.runtime.playback.setRate(this.cached(active.channel.source).rate);
        void this.track(
          active.runtime.playback
            .playFrom(active.startSeconds)
            .then(() => {
              if (!this.current(active)) return;
              if (!active.runtime.hasPlayRequest()) {
                this.stop();
                return;
              }
              // Native preparation and the initial seek survived; only now replace resume metadata.
              active.initial = false;
              this.observe(active);
            })
            .catch((reason) => {
              if (this.current(active)) {
                this.stop();
                this.failure(active.channel, reason);
              }
            }),
        );
      } else this.stop();
      return;
    }
    this.publish(
      active.channel,
      active.initial
        ? {
            ...state,
            loaded: !state.loadFailed,
            busy: !state.loadFailed,
            currentSeconds: active.startSeconds,
            rate: this.cached(active.channel.source).rate,
          }
        : state,
    );
    if (!active.initial && !active.validating) this.remember(active.channel);
  }
  pause(channel: MessageAudioChannel) {
    if (this.preparing?.channel.key === channel.key) {
      this.stop();
      return;
    }
    const active = this.active;
    if (active?.channel.key !== channel.key) return;
    this.cancelIntent(active.validationIntent);
    active.validationIntent = null;
    if (active.initial) {
      this.stop();
      return;
    }
    active.runtime.playback.pause();
  }
  seek(channel: MessageAudioChannel, seconds: number): Promise<void> {
    if (!Number.isFinite(seconds) || !this.admitted(channel.source)) return Promise.resolve();
    const active = this.active;
    if (active?.channel.key === channel.key && this.current(active))
      return active.runtime.playback.seekSeconds(seconds);
    if (active?.channel.key === channel.key) return Promise.resolve();
    const cached = this.cached(channel.source);
    this.remember(channel, {
      ...snapshot(cached),
      currentSeconds: Math.max(0, Math.min(cached.duration, seconds)),
    });
    this.publish(channel, snapshot(this.cached(channel.source)));
    return Promise.resolve();
  }
  rate(channel: MessageAudioChannel, rate: 1 | 1.5 | 2) {
    if (![1, 1.5, 2].includes(rate) || !this.admitted(channel.source)) return;
    const active = this.active;
    if (active?.channel.key === channel.key && this.current(active))
      active.runtime.playback.setRate(rate);
    else if (active?.channel.key !== channel.key) {
      this.remember(channel, { ...snapshot(this.cached(channel.source)), rate });
      this.publish(channel, snapshot(this.cached(channel.source)));
    }
  }
  retry(channel: MessageAudioChannel) {
    if (!this.admitted(channel.source)) return;
    if (this.active?.channel.key === channel.key) this.stop();
    void this.play(channel).catch((reason) => this.failure(channel, reason));
  }
  /** Coalesce coarse space events; no whole timeline refresh and no unguarded auto-resume. */
  changed(spaceIds?: readonly string[]) {
    const active = this.active;
    const source = active?.channel.source ?? this.preparing?.channel.source;
    if (!source || (spaceIds?.length && !spaceIds.includes(source.spaceId))) return;
    ++this.changes;
    if (!active) return;
    if (active.validating) {
      active.validationAgain = true;
      return;
    }
    active.validating = true;
    const resume =
      active.runtime.hasPlayRequest() ||
      active.runtime.playback.getSnapshot().playing ||
      active.runtime.playback.getSnapshot().busy;
    const first = active.firstPlay;
    active.firstPlay = null;
    active.runtime.playback.pause();
    active.validationIntent =
      first ??
      (resume
        ? this.intent(() => {
            if (this.active === active) active.runtime.playback.pause();
          })
        : null);
    void this.track(
      (async () => {
        try {
          let version: number | null;
          do {
            active.validationAgain = false;
            version = await this.verified(
              active.channel.source,
              () => this.active === active && this.admitted(active.channel.source),
            );
          } while (this.active === active && active.validationAgain);
          if (this.active !== active) return;
          if (version !== active.file.version || !this.admitted(active.channel.source))
            throw new Error('The audio message changed.');
          active.validating = false;
          const intent = active.validationIntent;
          active.validationIntent = null;
          if (intent && this.owns(intent)) {
            if (active.initial || !active.runtime.playback.getSnapshot().loaded) {
              active.firstPlay = intent;
              this.observe(active);
            } else {
              this.handoff(intent);
              await active.runtime.playback.playFrom();
            }
          } else this.cancelIntent(intent);
        } catch (reason) {
          if (this.active === active) {
            this.stop();
            this.failure(active.channel, reason);
          }
        }
      })(),
    ).catch(() => {});
  }
  private async drainRetired() {
    const results = await Promise.allSettled([...this.retired].map((value) => this.clean(value)));
    const failure = results.find((result) => result.status === 'rejected');
    if (failure?.status === 'rejected') throw failure.reason;
  }
  /** Retained cleanup is retryable; failure never authorizes closing the old core. */
  async retireSession() {
    this.retiring = true;
    this.blockedScope = scopeKey(this.port.scope()) ?? this.currentScope;
    this.stop();
    this.positions.clear();
    this.reconcile();
    while (this.work.size) await Promise.allSettled(this.work);
    await this.drainRetired();
  }
  /** Effect replay may reactivate this pure owner only after its previous resources drain. */
  async activate() {
    const lifecycle = ++this.lifecycle;
    this.disposed = false;
    this.retiring = true;
    while (this.work.size) await Promise.allSettled(this.work);
    await this.drainRetired();
    if (this.lifecycle !== lifecycle || this.disposed) return;
    this.retiring = false;
    this.blockedScope = null;
    this.reconcile();
  }
  async dispose() {
    const lifecycle = ++this.lifecycle;
    this.disposed = true;
    await this.retireSession();
    if (this.lifecycle !== lifecycle) return;
    this.focuses.length = 0;
    this.channels.clear();
  }
}
