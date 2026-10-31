import type { MediaDraftState, Stage, Write } from './media-draft-contract.ts';
import { mediaAcceptedIntent } from './media-acceptance.ts';
import {
  boundedText,
  contentValue,
  dtoValue,
  hasControl,
  id,
  invalid,
  MediaDraftFailure,
  target,
} from './media-draft-contract.ts';
import * as queuedWork from './media-draft-queued-work.ts';
import type {
  MaterializedMediaDraftAsset,
  MediaDraftContent,
  MediaDraftDto,
  MediaDraftIngest,
  MediaDraftPort,
  MediaDraftScope,
  MediaDraftSendResult,
} from './media-draft-types.ts';
export { MediaDraftFailure } from './media-draft-contract.ts';
export type { MediaDraftState } from './media-draft-contract.ts';

/** One scope's in-memory projection. Rust owns all durable metadata, bytes and receipts. */
export class MediaDraftChannel {
  readonly scope: MediaDraftScope;
  private readonly listeners = new Set<() => void>();
  private readonly port: MediaDraftPort;
  private readonly admitted: () => boolean;

  private readonly work = queuedWork.createMediaDraftWorkState();
  private readonly operations: queuedWork.MediaDraftWorkOwner;

  constructor(scope: MediaDraftScope, port: MediaDraftPort, admitted: () => boolean = () => true) {
    if (
      !boundedText(scope.expected_identity_id, 256) ||
      !scope.expected_identity_id ||
      !boundedText(scope.expected_device_id, 256) ||
      !scope.expected_device_id ||
      !boundedText(scope.space_id, 256) ||
      !scope.space_id ||
      !target(scope.thread_root_id)
    )
      invalid();
    this.scope = Object.freeze({ ...scope });
    this.port = port;
    this.admitted = admitted;
    this.operations = {
      work: this.work,
      scope: this.scope,
      port: this.port,
      accept: (value, preserveLocal) => this.accept(value, preserveLocal),
      cancelEdit: () => this.cancelEdit(),
      drainStages: (retry) => this.drainStages(retry),
      draining: () => this.draining(),
      enqueue: (work) => this.enqueue(work),
      flush: () => this.flush(),
      flushWrites: () => this.flushWrites(),
      load: () => this.load(),
      loaded: () => this.loaded(),
      notify: () => this.notify(),
      open: () => this.open(),
      publish: (patch) => this.publish(patch),
      pump: () => this.pump(),
      reconcile: (write) => this.reconcile(write),
      release: (uri) => this.release(uri),
      releaseDeferred: () => this.releaseDeferred(),
      saveAdmitted: (content, assets, stage, clearing) =>
        this.saveAdmitted(content, assets, stage, clearing),
      scheduleRelease: () => this.scheduleRelease(),
      send: () => this.send(),
      startStage: (stage) => this.startStage(stage),
    };
  }
  getSnapshot = (): MediaDraftState => this.work.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<MediaDraftState>) {
    if (this.work.retirement === 'discard') return;
    const next = { ...this.work.state, ...patch };
    this.work.state = Object.freeze({
      ...next,
      frozen:
        next.phase === 'closed' ||
        next.sending ||
        next.reviewing ||
        this.work.clearing ||
        this.work.recovering ||
        !!this.work.intent ||
        !!next.value?.attempted,
    });
    this.notify();
  }
  private notify() {
    for (const listener of this.listeners) {
      try {
        listener();
      } catch {
        // A control observer cannot interrupt native edit cancellation or file cleanup.
      }
    }
  }
  private open() {
    if (this.work.retirement || !this.admitted()) throw new MediaDraftFailure('closed');
  }
  private draining() {
    if (this.work.retirement === 'discard' || (!this.work.retirement && !this.admitted()))
      throw new MediaDraftFailure('closed');
  }
  private loaded() {
    if (!this.work.state.value) throw this.work.state.error ?? new MediaDraftFailure('notLoaded');
  }
  private enqueue<T>(work: () => Promise<T>): Promise<T> {
    const task = this.work.tail.then(work);
    this.work.tail = task.then(
      () => {},
      () => {},
    );
    return task;
  }
  private accept(value: MediaDraftDto, preserveLocal = false) {
    const parsed = dtoValue(value);
    this.publish({
      value: parsed.value,
      ...(preserveLocal ? {} : { content: parsed.content }),
      error: null,
    });
    return parsed.value;
  }

  load(): Promise<void> {
    try {
      this.open();
    } catch (error) {
      return Promise.reject(error);
    }
    if (this.work.loading) return this.work.loading;
    if (
      this.work.state.value &&
      !this.work.intent &&
      !this.work.state.error &&
      !this.work.needsReload
    )
      return Promise.resolve();
    const task = this.enqueue(async () => {
      this.draining();
      try {
        const value = await this.port.load(this.scope);
        this.draining();
        this.accept(value, this.work.writes.length > 0);
        this.work.needsReload = false;
        this.publish({ phase: this.work.retirement ? 'closed' : 'ready' });
      } catch (cause) {
        const error = cause instanceof Error ? cause : new Error('Media draft read failed');
        this.publish({ error });
        throw error;
      }
    });
    this.work.loading = task;
    const settled = () => {
      if (this.work.loading === task) this.work.loading = null;
    };
    void task.then(settled, settled);
    return task;
  }

  /** Prepare a new immutable copy, never resume the old transport intent. */
  async recoverFailed(
    messageId: string,
    normalRevision: string,
    current: () => boolean,
  ): Promise<void> {
    this.open();
    if (
      !this.port.recoverFailed ||
      this.work.recovering ||
      this.work.intent ||
      this.work.state.sending ||
      this.work.clearing ||
      this.work.editing ||
      this.work.stages.size ||
      this.work.writes.length ||
      this.work.writeError
    )
      throw new MediaDraftFailure('busy');
    this.work.recovering = true;
    this.publish({ saving: true });
    try {
      await this.load();
      await this.enqueue(async () => {
        this.open();
        this.loaded();
        const baseline = this.work.state.value!;
        if (!current() || this.work.state.content || baseline.attempted || baseline.assets.length)
          throw new MediaDraftFailure('conflict');
        const result = await this.port.recoverFailed!(
          this.scope,
          messageId,
          baseline.revision,
          normalRevision,
        );
        this.open();
        if (
          !current() ||
          this.work.state.value !== baseline ||
          this.work.state.content ||
          this.work.writes.length
        )
          throw new MediaDraftFailure('conflict');
        this.accept(result);
      });
    } catch (cause) {
      // An ACK may be lost after the encrypted copy committed. A subsequent
      // explicit Retry/route load reads the journal; it never repeats this action.
      this.work.needsReload = true;
      this.publish({ error: cause instanceof Error ? cause : new MediaDraftFailure('conflict') });
      throw cause;
    } finally {
      this.work.recovering = false;
      this.publish({ saving: false });
    }
  }

  save(
    content: MediaDraftContent | null,
    assets: readonly MediaDraftIngest[] = [],
  ): Promise<MediaDraftDto> {
    if (content === null && this.work.intent) return this.clear();
    return this.saveAdmitted(content, assets);
  }
  private saveAdmitted(
    content: MediaDraftContent | null,
    assets: readonly MediaDraftIngest[] = [],
    stage?: Stage,
    clearing = false,
  ): Promise<MediaDraftDto> {
    try {
      if (stage) {
        if (!stage.active || stage.cancelled || !this.work.stages.has(stage))
          throw new MediaDraftFailure('closed');
        this.draining();
      } else if (clearing) this.draining();
      else this.open();
      this.loaded();
      if (this.work.recovering) throw new MediaDraftFailure('busy');
      if (this.work.clearing && !clearing) throw new MediaDraftFailure('busy');
      if ((this.work.state.sending || this.work.editing) && !stage)
        throw new MediaDraftFailure('busy');
      if (content !== null && (this.work.intent || this.work.state.value!.attempted))
        throw new MediaDraftFailure('attempted');
      if (content === null && this.work.intent && !clearing)
        throw new MediaDraftFailure('attempted');
      const parsed = contentValue(content);
      const known = new Set([
        ...(this.work.state.value?.assets.map((asset) => asset.id) ?? []),
        ...this.work.writes.flatMap((write) => write.refs),
      ]);
      const ingests = assets.map((asset) => {
        const allowed = new Set(['id', 'uri', 'name', 'mime', 'voice_source_id', 'voice_role']);
        if (Object.keys(asset).some((key) => !allowed.has(key))) invalid();
        if (
          !id(asset.id) ||
          !parsed.refs.includes(asset.id) ||
          !boundedText(asset.uri, 16_384) ||
          !asset.uri ||
          !boundedText(asset.name, 1024) ||
          !asset.name ||
          /[/\\]/.test(asset.name) ||
          !boundedText(asset.mime, 255) ||
          !asset.mime.includes('/') ||
          hasControl(asset.mime) ||
          (asset.voice_source_id === undefined) !== (asset.voice_role === undefined) ||
          (asset.voice_source_id !== undefined &&
            (!boundedText(asset.voice_source_id, 256) || !asset.voice_source_id)) ||
          (asset.voice_role !== undefined &&
            asset.voice_role !== 'source' &&
            asset.voice_role !== 'render')
        )
          return invalid();
        known.add(asset.id);
        return Object.freeze({ ...asset });
      });
      if (
        new Set(ingests.map((asset) => asset.id)).size !== ingests.length ||
        parsed.refs.some((ref) => !known.has(ref))
      )
        invalid();
      const write: Write = {
        ...parsed,
        assets: Object.freeze(ingests),
        baseline: null,
        result: null,
        acceptedRevision: null,
      };
      this.work.writes.push(write);
      if (stage) stage.wrote = true;
      this.publish({ content: parsed.content });
      if (this.work.writeError && !stage) return Promise.reject(this.work.writeError);
      return this.pump().then(() => write.result!);
    } catch (cause) {
      return Promise.reject(cause);
    }
  }
  clear(): Promise<MediaDraftDto> {
    if (this.work.clearTask) return this.work.clearTask;
    try {
      this.open();
      this.loaded();
      if (this.work.recovering) throw new MediaDraftFailure('busy');
    } catch (error) {
      return Promise.reject(error);
    }
    const sending = this.work.sendTask;
    const original =
      this.work.intent ?? (sending ? { revision: this.work.state.value!.revision } : null);
    this.work.clearing = true;
    const cancelled = [...this.work.stages];
    for (const stage of cancelled) stage.cancelled = true;
    this.publish({ error: null });
    const task = (async () => {
      // Cancel retires preparation before waiting. An old prepare callback can
      // finish its native IO, but cannot save behind the explicit clear.
      await this.cancelEdit();
      await Promise.allSettled(cancelled.flatMap((stage) => (stage.task ? [stage.task] : [])));
      for (const stage of cancelled) this.work.stages.delete(stage);
      // An explicit discard waits an already admitted send; it never retries it.
      if (sending) await sending.catch(() => {});
      const intent = this.work.intent ?? original;
      await this.flushWrites();
      if (!intent) return this.saveAdmitted(null, [], undefined, true);
      const value = await this.enqueue(async () => {
        this.draining();
        const current = dtoValue(await this.port.load(this.scope)).value;
        this.draining();
        return this.accept(current);
      });
      // The ambiguous send may already have consumed its intent. Explicitly
      // abandoning that intent cannot erase a separately newer current draft.
      if (value.revision !== intent.revision) {
        this.work.intent = null;
        return value;
      }
      const cleared = await this.saveAdmitted(null, [], undefined, true);
      this.work.intent = null;
      return cleared;
    })();
    this.work.clearTask = task;
    const settled = () => {
      this.work.clearTask = null;
      this.work.clearing = false;
      this.publish({});
      this.scheduleRelease();
    };
    void task.then(settled, (cause: unknown) => {
      settled();
      this.publish({ error: cause instanceof Error ? cause : new Error('Media clear failed') });
    });
    return task;
  }
  stage(work: (save: MediaDraftChannel['save']) => Promise<void>): Promise<void> {
    return queuedWork.stage(this.operations, work);
  }

  private startStage(stage: Stage): Promise<void> {
    return queuedWork.startStage(this.operations, stage);
  }

  private drainStages(retry: boolean) {
    return queuedWork.drainStages(this.operations, retry);
  }

  private reconcile(write: Write) {
    return queuedWork.reconcile(this.operations, write);
  }

  private pump(): Promise<void> {
    return queuedWork.pump(this.operations);
  }

  private flushWrites() {
    return queuedWork.flushWrites(this.operations);
  }

  flush(): Promise<void> {
    return queuedWork.flush(this.operations);
  }

  retry(): Promise<void> {
    return queuedWork.retry(this.operations);
  }

  send(): Promise<MediaDraftSendResult> {
    if (this.work.sendTask) return this.work.sendTask;
    try {
      this.open();
      this.loaded();
      if (this.work.clearing || this.work.editing || this.work.recovering || this.work.needsReload)
        throw new MediaDraftFailure('busy');
    } catch (error) {
      return Promise.reject(error);
    }
    this.publish({ sending: true });
    const task = (async () => {
      try {
        await this.drainStages(false);
        await this.flushWrites();
        this.draining();
        const intent = this.work.intent ?? { revision: this.work.state.value!.revision };
        this.work.intent = intent;
        return await this.enqueue(async () => {
          this.draining();
          const result = await this.port.send(this.scope, intent.revision);
          this.draining();
          if (!Array.isArray(result.entries)) invalid();
          const candidate = dtoValue(result.draft).value;
          const receipt = mediaAcceptedIntent(candidate);
          if (
            receipt
              ? receipt.input_revision !== intent.revision ||
                result.entries.some((entry) => !receipt.messages.includes(entry.id))
              : !result.entries.length
          )
            invalid();
          const draft = this.accept(result.draft);
          this.work.intent = null;
          return Object.freeze({ entries: Object.freeze([...result.entries]), draft });
        });
      } catch (cause) {
        if (
          cause &&
          typeof cause === 'object' &&
          'code' in cause &&
          cause.code === 'ERR_ZOEN_DRAFT_REJECTED'
        )
          this.work.intent = null;
        const error = cause instanceof Error ? cause : new Error('Media draft send failed');
        this.publish({ error });
        throw error;
      }
    })();
    this.work.sendTask = task;
    const settled = () => {
      if (this.work.sendTask === task) this.work.sendTask = null;
      this.publish({ sending: false });
      this.scheduleRelease();
    };
    void task.then(settled, settled);
    return task;
  }
  /** One writer barrier around native review; Rust remains the only content authority. */
  edit(assetId: string, presentationId: string): Promise<void> {
    try {
      this.open();
      this.loaded();
      if (!this.port.edit || !this.port.cancelPreview) throw new MediaDraftFailure('invalid');
      if (
        this.work.editing ||
        this.work.clearing ||
        this.work.state.sending ||
        this.work.stages.size ||
        this.work.intent ||
        this.work.state.value?.attempted
      )
        throw new MediaDraftFailure('busy');
      const content = this.work.state.content;
      if (
        content?.kind !== 'attachments' ||
        !content.items.some((item) => item.asset_id === assetId)
      )
        throw new MediaDraftFailure('invalid');
    } catch (error) {
      return Promise.reject(error);
    }
    const editing = { token: presentationId, task: Promise.resolve() };
    this.work.editing = editing;
    const task = this.enqueue(async () => {
      this.draining();
      // prepare() flushed all prior writes before the presentation was admitted.
      if (this.work.writes.length || this.work.writeError) throw new MediaDraftFailure('busy');
      const baseline = this.work.state.value!;
      try {
        const result = await this.port.edit!(
          this.scope,
          baseline.revision,
          assetId,
          presentationId,
        );
        this.draining();
        this.accept(result);
      } catch (error) {
        // Save may have won just before Cancel, or its bridge ACK may have been lost.
        // Re-read Rust rather than restoring an obsolete optimistic original.
        this.draining();
        const recovered = await this.port.load(this.scope);
        this.draining();
        this.accept(recovered);
        throw error;
      }
    });
    editing.task = task;
    this.publish({ reviewing: true, error: null });
    const settled = () => {
      if (this.work.editing === editing) this.work.editing = null;
      this.publish({ reviewing: false });
      this.scheduleRelease();
    };
    void task.then(settled, (reason: unknown) => {
      settled();
      if (!this.work.retirement)
        this.publish({
          error: reason instanceof Error ? reason : new Error('Media review failed'),
        });
    });
    return task;
  }
  private async cancelEdit(): Promise<void> {
    const editing = this.work.editing;
    if (!editing) return;
    await this.port.cancelPreview!(editing.token);
    await editing.task.catch(() => {});
  }
  materialize(assetId: string): Promise<MaterializedMediaDraftAsset> {
    return queuedWork.materialize(this.operations, assetId);
  }

  release(uri: string): Promise<void> {
    return queuedWork.release(this.operations, uri);
  }

  deferRelease(work: () => Promise<void>): void {
    return queuedWork.deferRelease(this.operations, work);
  }

  private scheduleRelease() {
    return queuedWork.scheduleRelease(this.operations);
  }

  private releaseDeferred(): Promise<void> {
    return queuedWork.releaseDeferred(this.operations);
  }

  retire(discard: boolean): Promise<void> {
    return queuedWork.retire(this.operations, discard);
  }
}
