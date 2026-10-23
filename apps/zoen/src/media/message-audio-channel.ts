import type { AudioClipPlayback, AudioClipPlaybackState } from './audio-clip-controller.ts';
import { sourceKey, type MessageAudioSource } from './message-audio-contract.ts';
import type { MessageAudioSession } from './message-audio-session.ts';

/** Controls have no native player or decrypted URI, and may outlive a recycled row. */
export class MessageAudioChannel implements AudioClipPlayback {
  readonly source: MessageAudioSource;
  readonly key: string;
  private readonly session: MessageAudioSession;
  private state: AudioClipPlaybackState;
  private readonly listeners = new Set<() => void>();
  constructor(session: MessageAudioSession, source: MessageAudioSource) {
    this.session = session;
    this.source = Object.freeze({ ...source, scope: Object.freeze({ ...source.scope }) });
    this.key = sourceKey(this.source);
    this.state = session.initial(this.source);
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    this.session.attach(this);
    return () => {
      this.listeners.delete(listener);
      if (!this.listeners.size) this.session.detach(this);
    };
  };
  publish(state: AudioClipPlaybackState) {
    if (
      Object.keys(state).every((key) =>
        Object.is(
          state[key as keyof AudioClipPlaybackState],
          this.state[key as keyof AudioClipPlaybackState],
        ),
      )
    )
      return;
    this.state = Object.freeze({ ...state });
    for (const listener of this.listeners) {
      try {
        listener();
      } catch {
        // Observer failures cannot interrupt player/file cleanup or updates to
        // other controls.
      }
    }
  }
  playFrom = (seconds?: number) => this.session.play(this, seconds);
  pause = () => this.session.pause(this);
  invalidate = () => this.pause();
  seekSeconds = (seconds: number) => this.session.seek(this, seconds);
  setRate = (rate: 1 | 1.5 | 2) => this.session.rate(this, rate);
  retry = () => this.session.retry(this);
}
