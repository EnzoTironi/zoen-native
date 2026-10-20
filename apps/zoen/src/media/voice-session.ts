import {
  attachVoiceTranscript,
  createVoiceDraft,
  editVoiceDraft,
  MAX_VOICE_SECONDS,
  MAX_VOICE_WAVEFORM_BARS,
  MIN_VOICE_MILLISECONDS,
  projectVoiceDraft,
  redoVoiceEdit,
  undoVoiceEdit,
  voiceRenderPlan,
  type FrameRange,
  type VoiceDraft,
  type VoiceEditCommand,
  type VoiceProjection,
  type VoiceSource,
  type VoiceTranscript,
} from './voice-edit.ts';
import {
  restoreVoiceEditor,
  voiceEditorState,
  type VoiceEditorState,
} from './voice-editor-state.ts';

export type VoicePreparedSource = VoiceSource & Readonly<{ uri: string; durationMs: number }>;
export type VoiceRenderResult = Readonly<{
  uri: string;
  mime: string;
  durationMs: number;
  frameCount: number;
  levels: readonly number[];
}>;
export type VoiceTranscription = VoiceTranscript & Readonly<{ status: 'ready' | 'unavailable' }>;
export interface VoiceNativePort {
  prepare(uri: string): Promise<VoicePreparedSource>;
  render(sourceId: string, ranges: readonly FrameRange[]): Promise<VoiceRenderResult>;
  transcribe(sourceId: string, locale: string): Promise<VoiceTranscription>;
  releaseRender(sourceId: string, uri: string): Promise<void>;
  cancel(sourceId: string): Promise<void>;
  release(sourceId: string): Promise<void>;
}
export type VoiceSessionFailureKind =
  | 'prepare'
  | 'render'
  | 'transcribe'
  | 'cleanup'
  | 'invalid_render'
  | 'closed'
  | 'not_ready';
export class VoiceSessionFailure extends Error {
  readonly kind: VoiceSessionFailureKind;
  constructor(kind: VoiceSessionFailureKind, cause?: unknown) {
    super(kind, { cause });
    this.name = 'VoiceSessionFailure';
    this.kind = kind;
  }
}
export type VoiceSessionState = Readonly<{
  phase: 'idle' | 'preparing' | 'ready' | 'closing' | 'closed';
  source: VoicePreparedSource | null;
  draft: VoiceDraft | null;
  projection: VoiceProjection | null;
  preview: VoiceRenderResult | null;
  rendering: boolean;
  transcribing: boolean;
  transcription: 'idle' | 'ready' | 'unavailable';
  canSend: boolean;
  error: VoiceSessionFailure | null;
  pendingTrim: FrameRange | null;
  attempted: boolean;
}>;
export type VoiceSendable = Readonly<{ preview: VoiceRenderResult; transcript: string }>;
type Lease = { id: string; uri: string; renders: Set<string>; released: boolean };
const initialState = (): VoiceSessionState =>
  Object.freeze({
    phase: 'idle',
    source: null,
    draft: null,
    projection: null,
    preview: null,
    rendering: false,
    transcribing: false,
    transcription: 'idle',
    canSend: false,
    error: null,
    pendingTrim: null,
    attempted: false,
  });

/** One recording's local lease. It owns no account, files, React state, or send mutation. */
export class VoiceSession {
  private state = initialState();
  private readonly listeners = new Set<() => void>();
  private readonly pending = new Set<Promise<unknown>>();
  private readonly leases = new Map<string, Lease>();
  private generation = 0;
  private revision = 0;
  private previewRevision = -1;
  private dirty = false;
  private rendering: Promise<void> | null = null;
  private transcribing: Promise<void> | null = null;
  private closing: Promise<void> | null = null;
  private readonly port: VoiceNativePort;

  constructor(port: VoiceNativePort) {
    this.port = port;
  }
  getSnapshot = (): VoiceSessionState => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<VoiceSessionState>) {
    this.state = Object.freeze({ ...this.state, ...patch });
    for (const listener of this.listeners) {
      try {
        listener();
      } catch {
        // Control notifications cannot interrupt native cancellation or lease cleanup.
      }
    }
  }
  private track<T>(task: Promise<T>): Promise<T> {
    this.pending.add(task);
    void task.finally(() => this.pending.delete(task)).catch(() => {});
    return task;
  }
  private current(generation: number, id?: string) {
    return (
      generation === this.generation &&
      this.state.phase !== 'closing' &&
      this.state.phase !== 'closed' &&
      (id === undefined || this.state.source?.id === id)
    );
  }
  private failure(kind: VoiceSessionFailureKind, cause: unknown) {
    const error =
      cause instanceof VoiceSessionFailure ? cause : new VoiceSessionFailure(kind, cause);
    this.publish({ error, canSend: false });
  }
  prepare = (
    uri: string,
    options: Readonly<{
      editor?: unknown;
      logicalSourceId?: string;
      attempted?: boolean;
      frozenPreview?: VoiceRenderResult;
      beforeReady?: (source: VoicePreparedSource, editor: VoiceEditorState) => Promise<void>;
    }> = {},
  ): Promise<void> => {
    if (this.state.phase === 'closed' || this.state.phase === 'closing')
      return Promise.reject(new VoiceSessionFailure('closed'));
    if (this.state.phase === 'ready') return Promise.reject(new VoiceSessionFailure('not_ready'));
    const generation = ++this.generation;
    this.publish({ ...initialState(), phase: 'preparing' });
    return this.track(
      (async () => {
        let lease: Lease | undefined;
        try {
          const prepared = await this.port.prepare(uri);
          // Capture the native lease before metadata validation so even malformed results get cleaned.
          if (typeof prepared.id === 'string' && prepared.id && !this.leases.has(prepared.id)) {
            lease = { id: prepared.id, uri: prepared.uri, renders: new Set(), released: false };
            this.leases.set(lease.id, lease);
          }
          if (!lease) throw new VoiceSessionFailure('prepare');
          if (!this.current(generation)) {
            await this.releaseLease(lease);
            return;
          }
          const restored = options.editor
            ? restoreVoiceEditor(options.editor, prepared)
            : voiceEditorState(
                createVoiceDraft({ ...prepared, id: options.logicalSourceId ?? prepared.id }),
                null,
              );
          const draft = restored.draft;
          if (
            typeof prepared.uri !== 'string' ||
            !prepared.uri ||
            !Number.isFinite(prepared.durationMs) ||
            prepared.durationMs <= 0 ||
            prepared.durationMs > MAX_VOICE_SECONDS * 1000
          )
            throw new VoiceSessionFailure('prepare');
          const source = Object.freeze({
            ...prepared,
            levels: Object.freeze([...prepared.levels]),
            uri: prepared.uri,
            durationMs: prepared.durationMs,
          });
          this.revision = 0;
          this.previewRevision = -1;
          this.publish({ source, draft, pendingTrim: restored.pendingTrim });
          await options.beforeReady?.(source, restored);
          if (!this.current(generation, source.id)) return;
          this.publish({
            phase: 'ready',
            source,
            draft,
            projection: projectVoiceDraft(draft),
            error: null,
            pendingTrim: restored.pendingTrim,
            attempted: options.attempted ?? false,
          });
          if (options.attempted && !options.frozenPreview)
            throw new VoiceSessionFailure('invalid_render');
          if (options.attempted && options.frozenPreview) {
            const projection = projectVoiceDraft(draft);
            const preview = options.frozenPreview;
            if (
              preview.frameCount !== projection.frameCount ||
              preview.mime !== 'audio/mp4' ||
              Math.abs(preview.durationMs - projection.durationMs) > 1
            )
              throw new VoiceSessionFailure('invalid_render');
            this.previewRevision = this.revision;
            this.publish({ preview, canSend: projection.canSend });
          } else await this.retryPreview();
        } catch (cause) {
          if (lease && this.state.source?.id !== lease.id) {
            try {
              await this.releaseLease(lease);
            } catch (cleanup) {
              if (this.current(generation)) this.failure('cleanup', cleanup);
            }
          }
          if (this.current(generation)) {
            this.publish({ phase: 'idle' });
            this.failure('prepare', cause);
          }
        }
      })(),
    );
  };
  private change(draft: VoiceDraft) {
    if (draft === this.state.draft) return;
    ++this.revision;
    this.publish({
      draft,
      projection: projectVoiceDraft(draft),
      pendingTrim: null,
      canSend: false,
      error: null,
    });
    void this.retryPreview();
  }
  private readyDraft(allowPendingTrim = false): VoiceDraft {
    if (
      this.state.phase !== 'ready' ||
      !this.state.draft ||
      this.state.attempted ||
      (!allowPendingTrim && this.state.pendingTrim)
    )
      throw new VoiceSessionFailure('not_ready');
    return this.state.draft;
  }
  canEdit = (commands: VoiceEditCommand | readonly VoiceEditCommand[]): boolean => {
    if (this.state.phase !== 'ready' || !this.state.draft || this.state.attempted) return false;
    const pending = this.state.pendingTrim;
    if (!pending) return true;
    const edits = Array.isArray(commands) ? commands : [commands];
    const command = edits.length === 1 ? edits[0] : null;
    // A precision preview is a transaction: only its exact Apply may replace
    // canonical edits. Other commands wait for explicit Apply or Discard.
    return (
      command?.type === 'trim' &&
      command.range.startFrame === pending.startFrame &&
      command.range.endFrame === pending.endFrame
    );
  };
  edit = (commands: VoiceEditCommand | readonly VoiceEditCommand[]) => {
    if (!this.canEdit(commands)) throw new VoiceSessionFailure('not_ready');
    this.change(editVoiceDraft(this.readyDraft(true), commands));
  };
  undo = () => {
    this.change(undoVoiceEdit(this.readyDraft()));
  };
  redo = () => {
    this.change(redoVoiceEdit(this.readyDraft()));
  };
  editorState = (): VoiceEditorState | null =>
    this.state.draft ? voiceEditorState(this.state.draft, this.state.pendingTrim) : null;
  setPendingTrim = (draft: VoiceDraft, pending: FrameRange | null) => {
    if (this.state.phase !== 'ready' || this.state.draft !== draft || this.state.attempted) return;
    const value = voiceEditorState(draft, pending).pendingTrim;
    const current = this.state.pendingTrim;
    if (current?.startFrame === value?.startFrame && current?.endFrame === value?.endFrame) return;
    this.publish({ pendingTrim: value });
  };
  freeze = (attempted = true) => this.publish({ attempted });

  private async cleanRenders(lease: Lease, keep?: string) {
    for (const uri of lease.renders) {
      if (uri === keep) continue;
      try {
        await this.port.releaseRender(lease.id, uri);
      } catch (cause) {
        throw new VoiceSessionFailure('cleanup', cause);
      }
      lease.renders.delete(uri);
    }
  }
  retryPreview = (): Promise<void> => {
    if (this.state.phase !== 'ready' || this.state.attempted) return Promise.resolve();
    this.dirty = true;
    if (this.rendering) return this.rendering;
    this.publish({ rendering: true, canSend: false, error: null });
    const task = this.track(
      (async () => {
        while (this.dirty && this.state.phase === 'ready') {
          this.dirty = false;
          const source = this.state.source!;
          const generation = this.generation;
          const revision = this.revision;
          const lease = this.leases.get(source.id)!;
          try {
            await this.cleanRenders(lease, this.state.preview?.uri);
            if (!this.current(generation, source.id)) return;
            if (revision !== this.revision) {
              this.dirty = true;
              continue;
            }
            const projection = this.state.projection!;
            if (!projection.canSend) continue;
            if (this.state.preview && this.previewRevision === revision) {
              this.publish({ canSend: true, error: null });
              continue;
            }
            const plan = voiceRenderPlan(this.state.draft!);
            const result = await this.port.render(source.id, plan.keptRanges);
            const reusedUri = lease.renders.has(result.uri);
            if (typeof result.uri === 'string' && result.uri && result.uri !== source.uri)
              lease.renders.add(result.uri);
            if (!this.current(generation, source.id) || revision !== this.revision) {
              await this.cleanRenders(lease, this.state.preview?.uri);
              continue;
            }
            if (
              typeof result.uri !== 'string' ||
              !result.uri ||
              result.uri === source.uri ||
              reusedUri ||
              result.mime !== 'audio/mp4' ||
              !Number.isSafeInteger(result.frameCount) ||
              result.frameCount !== plan.frameCount ||
              !Number.isFinite(result.durationMs) ||
              result.durationMs < MIN_VOICE_MILLISECONDS ||
              result.durationMs > MAX_VOICE_SECONDS * 1000 ||
              !Array.isArray(result.levels) ||
              result.levels.length > MAX_VOICE_WAVEFORM_BARS ||
              result.levels.some((level) => !Number.isFinite(level) || level < 0 || level > 1)
            )
              throw new VoiceSessionFailure('invalid_render');
            const preview = Object.freeze({
              uri: result.uri,
              mime: result.mime,
              durationMs: result.durationMs,
              frameCount: result.frameCount,
              levels: Object.freeze([...result.levels]),
            });
            this.previewRevision = revision;
            this.publish({ preview, canSend: false });
            await this.cleanRenders(lease, preview.uri);
            if (this.current(generation, source.id) && revision === this.revision)
              this.publish({ canSend: true, error: null });
          } catch (cause) {
            if (this.current(generation, source.id) && revision !== this.revision && this.dirty)
              continue;
            if (this.current(generation, source.id)) this.failure('render', cause);
            this.dirty = false;
          }
        }
      })(),
    );
    this.rendering = task;
    void task
      .finally(() => {
        if (this.rendering === task) {
          this.rendering = null;
          if (this.state.phase === 'ready') this.publish({ rendering: false });
        }
      })
      .catch(() => {});
    return task;
  };
  transcribe = (locale: string): Promise<void> => {
    if (this.state.phase !== 'ready' || this.state.attempted) return Promise.resolve();
    if (this.transcribing) return this.transcribing;
    if (this.state.draft?.transcript) return Promise.resolve();
    const source = this.state.source!;
    const generation = this.generation;
    this.publish({ transcribing: true, error: null });
    const task = this.track(
      (async () => {
        try {
          const result = await this.port.transcribe(source.id, locale);
          if (!this.current(generation, source.id)) return;
          if (result.status === 'unavailable') {
            this.publish({ transcription: 'unavailable' });
            return;
          }
          if (result.status !== 'ready') throw new VoiceSessionFailure('transcribe');
          if (this.state.attempted) return;
          const draft = attachVoiceTranscript(
            this.state.draft!,
            this.state.draft!.original.id,
            result,
          );
          this.publish({ draft, projection: projectVoiceDraft(draft), transcription: 'ready' });
        } catch (cause) {
          if (this.current(generation, source.id))
            this.publish({ error: new VoiceSessionFailure('transcribe', cause) });
        }
      })(),
    );
    this.transcribing = task;
    void task
      .finally(() => {
        if (this.transcribing === task) {
          this.transcribing = null;
          if (this.state.phase === 'ready') this.publish({ transcribing: false });
        }
      })
      .catch(() => {});
    return task;
  };
  getSendable = (): VoiceSendable | null => {
    const { phase, canSend, preview, projection } = this.state;
    return phase === 'ready' &&
      !this.state.pendingTrim &&
      canSend &&
      preview &&
      projection &&
      this.previewRevision === this.revision
      ? Object.freeze({ preview, transcript: projection.transcript })
      : null;
  };
  private async releaseLease(lease: Lease) {
    if (lease.released) return;
    // release must still run when cancellation fails; it owns the final native cleanup.
    try {
      await this.port.cancel(lease.id);
    } catch {
      /* release below retries cancellation natively. */
    }
    await this.port.release(lease.id);
    lease.renders.clear();
    lease.released = true;
  }
  close = (): Promise<void> => {
    if (this.state.phase === 'closed') return Promise.resolve();
    if (this.closing) return this.closing;
    ++this.generation;
    this.dirty = false;
    this.publish({ phase: 'closing', canSend: false, error: null });
    const task = (async () => {
      await Promise.allSettled(
        [...this.leases.values()]
          .filter((lease) => !lease.released)
          .map((lease) => this.port.cancel(lease.id)),
      );
      while (this.pending.size) await Promise.allSettled(this.pending);
      const results = await Promise.allSettled(
        [...this.leases.values()].map((lease) => this.releaseLease(lease)),
      );
      const failures = results.flatMap((result) =>
        result.status === 'rejected' ? [result.reason] : [],
      );
      if (failures.length) {
        const error = new VoiceSessionFailure('cleanup', new AggregateError(failures));
        this.publish({ error });
        throw error;
      }
      this.leases.clear();
      this.publish({ ...initialState(), phase: 'closed' });
    })();
    this.closing = task;
    void task
      .finally(() => {
        if (this.closing === task) this.closing = null;
      })
      .catch(() => {});
    return task;
  };
}
