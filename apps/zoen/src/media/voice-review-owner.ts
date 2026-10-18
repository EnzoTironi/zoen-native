import type { SensitiveCacheWork } from './cache-work.ts';
import {
  VoiceSession,
  VoiceSessionFailure,
  type VoicePreparedSource,
  type VoiceRenderResult,
  type VoiceNativePort,
  type VoiceSendable,
} from './voice-session.ts';
import type { VoiceEditorState } from './voice-editor-state.ts';

type Options = {
  port: VoiceNativePort;
  uri: string;
  registerCleanup: (cleanup: () => Promise<void>) => () => void;
  work: Pick<SensitiveCacheWork, 'run' | 'track'>;
  discard: (uri: string) => Promise<void> | void;
  editor?: unknown;
  logicalSourceId?: string;
  attempted?: boolean;
  frozenPreview?: VoiceRenderResult;
  beforeReady?: (source: VoicePreparedSource, editor: VoiceEditorState) => Promise<void>;
  releaseMaterialized?: () => Promise<void>;
};

/** Created once in the capture/import action, never during render or an effect replay. */
export class VoiceReviewOwner {
  readonly session: VoiceSession;
  private readonly options: Options;
  private readonly unregister: () => void;
  private accepting = true;
  private committed = false;
  private closed = false;
  private sending: Promise<void> | null = null;
  private closing: Promise<void> | null = null;

  constructor(options: Options) {
    this.options = options;
    this.session = new VoiceSession(options.port);
    this.unregister = options.registerCleanup(() => this.close());
  }

  get uri() {
    return this.options.uri;
  }

  prepare = (): Promise<void> => {
    if (!this.accepting || this.committed || this.sending)
      return Promise.reject(new VoiceSessionFailure('closed'));
    return this.options.work.run(() => this.session.prepare(this.options.uri, this.options));
  };

  async send<T>(captured: VoiceSendable, commit: (value: VoiceSendable) => Promise<T>): Promise<T> {
    if (!this.accepting || this.committed || this.sending) throw new VoiceSessionFailure('closed');
    const current = this.session.getSendable();
    if (
      !current ||
      current.preview !== captured.preview ||
      current.transcript !== captured.transcript
    )
      throw new VoiceSessionFailure('not_ready');
    let finish!: () => void;
    // Register the in-flight mutation before invoking external code, including synchronous callbacks.
    this.sending = new Promise<void>((resolve) => {
      finish = resolve;
    });
    try {
      const result = await this.options.work.run(() => commit(captured));
      this.committed = true;
      return result;
    } finally {
      finish();
      this.sending = null;
    }
  }

  close = (): Promise<void> => {
    this.accepting = false;
    if (this.closed) return Promise.resolve();
    if (this.closing) return this.closing;
    const task = this.options.work.track(
      (async () => {
        if (this.sending) await this.sending;
        await this.session.close();
        await this.options.discard(this.options.uri);
        await this.options.releaseMaterialized?.();
        this.closed = true;
        // Failed native/source cleanup stays registered for device-erasure retries.
        this.unregister();
      })(),
    );
    this.closing = task;
    void task
      .finally(() => {
        this.closing = null;
      })
      .catch(() => {});
    return task;
  };
}
