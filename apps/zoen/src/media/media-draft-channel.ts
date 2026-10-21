import type {
  MaterializedMediaDraftAsset,
  MediaDraftAsset,
  MediaDraftAttachmentOptions,
  MediaDraftContent,
  MediaDraftDto,
  MediaDraftIngest,
  MediaDraftPort,
  MediaDraftScope,
  MediaDraftSendResult,
} from './media-draft-types.ts';

export type MediaDraftState = Readonly<{
  phase: 'loading' | 'ready' | 'closed';
  value: MediaDraftDto | null;
  content: MediaDraftContent | null;
  saving: boolean;
  sending: boolean;
  reviewing: boolean;
  frozen: boolean;
  error: Error | null;
}>;
export class MediaDraftFailure extends Error {
  readonly kind: 'closed' | 'notLoaded' | 'invalid' | 'busy' | 'attempted' | 'conflict';
  constructor(kind: MediaDraftFailure['kind'], cause?: unknown) {
    super(cause instanceof Error ? cause.message : `Media draft ${kind}`, { cause });
    this.name = 'MediaDraftFailure';
    this.kind = kind;
  }
}
function invalid(): never {
  throw new MediaDraftFailure('invalid');
}
const bytes = (text: string) => {
  let count = 0;
  for (const letter of text) {
    const point = letter.codePointAt(0)!;
    count += point < 128 ? 1 : point < 2048 ? 2 : point < 65536 ? 3 : 4;
  }
  return count;
};
const id = (value: unknown): value is string =>
  typeof value === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(value);
const boundedText = (value: unknown, limit: number): value is string =>
  typeof value === 'string' && !value.includes('\0') && bytes(value) <= limit;
const hasControl = (value: string) =>
  [...value].some((char) => char.charCodeAt(0) < 32 || char.charCodeAt(0) === 127);
const target = (value: unknown): value is string | null =>
  value === null || (boundedText(value, 256) && value.length > 0);
const object = (value: unknown): Record<string, unknown> => {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return invalid();
  return value as Record<string, unknown>;
};
function keys(value: Record<string, unknown>, expected: string[]) {
  if (Object.keys(value).sort().join(',') !== expected.sort().join(',')) invalid();
}
function freeze<T>(value: T): T {
  if (value && typeof value === 'object') {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}
/** Opaque editor data remains JSON, never an SDK object, cache path or file lease. */
function editorJson(value: unknown, depth = 0, seen = new Set<unknown>()): unknown {
  if (depth > 64) invalid();
  if (value === null || typeof value === 'boolean') return value;
  if (typeof value === 'number') return Number.isFinite(value) ? value : invalid();
  if (typeof value === 'string') {
    if (/^(?:file:|content:|blob:|ph:|assets-library:|\/private\/|\/var\/|\/Users\/)/i.test(value))
      invalid();
    return value;
  }
  if (!value || typeof value !== 'object' || seen.has(value)) return invalid();
  const proto = Object.getPrototypeOf(value);
  if (!Array.isArray(value) && proto !== Object.prototype && proto !== null) invalid();
  seen.add(value);
  let result: unknown;
  if (Array.isArray(value)) result = value.map((child) => editorJson(child, depth + 1, seen));
  else {
    const record: Record<string, unknown> = Object.create(null) as Record<string, unknown>;
    for (const [key, child] of Object.entries(value).sort(([left], [right]) =>
      left.localeCompare(right),
    )) {
      if (/(?:^|_)(?:uri|url|path|lease)(?:$|_)|(?:Uri|URL|Path|Lease)$/.test(key)) invalid();
      record[key] = editorJson(child, depth + 1, seen);
    }
    result = record;
  }
  seen.delete(value);
  return result;
}
function contentValue(input: unknown): {
  content: MediaDraftContent | null;
  json: string;
  refs: string[];
} {
  if (input === null) return { content: null, json: 'null', refs: [] };
  const raw = object(input);
  const refs: string[] = [];
  if (!target(raw.reply_to)) invalid();
  let content: MediaDraftContent;
  if (raw.kind === 'attachments') {
    const hasOptions = Object.hasOwn(raw, 'options');
    keys(raw, ['kind', 'reply_to', 'items', ...(hasOptions ? ['options'] : [])]);
    if (!Array.isArray(raw.items) || !raw.items.length || raw.items.length > 10) invalid();
    const items = raw.items.map((item: unknown) => {
      const entry = object(item);
      keys(entry, ['asset_id', 'caption', 'mutation_id']);
      if (!id(entry.asset_id) || !id(entry.mutation_id) || !boundedText(entry.caption, 16 * 1024))
        return invalid();
      refs.push(entry.asset_id);
      return { asset_id: entry.asset_id, caption: entry.caption, mutation_id: entry.mutation_id };
    });
    if (new Set(items.map((item) => item.mutation_id)).size !== items.length) invalid();
    let options: MediaDraftAttachmentOptions | undefined;
    if (hasOptions) {
      const value = object(raw.options);
      keys(value, ['presentation', 'grouping']);
      if (
        (value.presentation !== 'media' && value.presentation !== 'file') ||
        (value.grouping !== 'separate' && value.grouping !== 'grouped') ||
        (value.grouping === 'grouped' && items.length < 2)
      )
        invalid();
      // Keep legacy content JSON unchanged so its durable save receipts remain valid.
      if (value.presentation !== 'media' || value.grouping !== 'separate')
        options = { presentation: value.presentation, grouping: value.grouping };
    }
    content = {
      kind: 'attachments',
      reply_to: raw.reply_to,
      items,
      ...(options ? { options } : {}),
    };
  } else if (raw.kind === 'voice') {
    keys(raw, ['kind', 'reply_to', 'source_asset_id', 'editor', 'render']);
    if (!id(raw.source_asset_id)) invalid();
    refs.push(raw.source_asset_id);
    let render = null;
    if (raw.render !== null) {
      const entry = object(raw.render);
      keys(entry, ['asset_id', 'mime', 'duration_ms', 'levels', 'transcript']);
      if (
        !id(entry.asset_id) ||
        !boundedText(entry.mime, 255) ||
        !entry.mime.startsWith('audio/') ||
        typeof entry.duration_ms !== 'number' ||
        !Number.isFinite(entry.duration_ms) ||
        entry.duration_ms < 250 ||
        entry.duration_ms > 3_600_000 ||
        !Array.isArray(entry.levels) ||
        entry.levels.length > 160 ||
        !entry.levels.every(
          (level) =>
            typeof level === 'number' && Number.isFinite(level) && level >= 0 && level <= 1,
        ) ||
        !boundedText(entry.transcript, 65536)
      )
        return invalid();
      refs.push(entry.asset_id);
      render = {
        asset_id: entry.asset_id,
        mime: entry.mime,
        duration_ms: entry.duration_ms,
        levels: [...entry.levels] as number[],
        transcript: entry.transcript,
      };
    }
    content = {
      kind: 'voice',
      reply_to: raw.reply_to,
      source_asset_id: raw.source_asset_id as string,
      editor: editorJson(raw.editor),
      render,
    };
  } else return invalid();
  if (new Set(refs).size !== refs.length) invalid();
  const json = JSON.stringify(content);
  if (bytes(json) > 512 * 1024) invalid();
  return { content: freeze(content), json, refs };
}
function assetValue(input: unknown): MediaDraftAsset {
  const value = object(input);
  keys(value, ['id', 'name', 'mime', 'size', 'sha256']);
  if (
    !id(value.id) ||
    !boundedText(value.name, 1024) ||
    !value.name.length ||
    /[/\\]/.test(value.name) ||
    !boundedText(value.mime, 255) ||
    !value.mime.includes('/') ||
    hasControl(value.mime) ||
    typeof value.size !== 'number' ||
    !Number.isSafeInteger(value.size) ||
    value.size <= 0 ||
    value.size > 64 * 1024 * 1024 ||
    typeof value.sha256 !== 'string' ||
    !/^[a-f0-9]{64}$/.test(value.sha256)
  )
    return invalid();
  return Object.freeze({
    id: value.id,
    name: value.name,
    mime: value.mime,
    size: value.size,
    sha256: value.sha256,
  });
}
function dtoValue(input: unknown): { value: MediaDraftDto; content: MediaDraftContent | null } {
  const raw = object(input);
  keys(raw, ['revision', 'content_json', 'assets', 'attempted']);
  if (
    !boundedText(raw.revision, 256) ||
    !raw.revision.length ||
    !boundedText(raw.content_json, 512 * 1024) ||
    !Array.isArray(raw.assets) ||
    typeof raw.attempted !== 'boolean'
  )
    return invalid();
  const parsed = contentValue(JSON.parse(raw.content_json));
  const assets = raw.assets.map(assetValue);
  if (
    assets.length !== parsed.refs.length ||
    assets.some((asset, index) => asset.id !== parsed.refs[index])
  )
    invalid();
  const total = assets.reduce((count, asset) => count + asset.size, 0);
  if (total > (parsed.content?.kind === 'voice' ? 128 : 64) * 1024 * 1024) invalid();
  if (parsed.content?.kind === 'voice') {
    if (assets.some((asset) => !asset.mime.startsWith('audio/'))) invalid();
    if (parsed.content.render && parsed.content.render.mime !== assets[1]?.mime) invalid();
  }
  if (
    (parsed.content === null && raw.attempted) ||
    (raw.revision === '0' && parsed.content !== null)
  )
    invalid();
  return {
    value: freeze({
      revision: raw.revision,
      content_json: parsed.json,
      assets,
      attempted: raw.attempted,
    }),
    content: parsed.content,
  };
}
function saveResultValue(input: unknown): { acceptedRevision: string; value: MediaDraftDto } {
  const raw = object(input);
  keys(raw, ['accepted_revision', 'draft']);
  if (!boundedText(raw.accepted_revision, 256) || !raw.accepted_revision.length) invalid();
  return { acceptedRevision: raw.accepted_revision, value: dtoValue(raw.draft).value };
}
type Write = {
  content: MediaDraftContent | null;
  json: string;
  refs: string[];
  assets: readonly MediaDraftIngest[];
  baseline: MediaDraftDto | null;
  result: MediaDraftDto | null;
  acceptedRevision: string | null;
};
type Intent = { revision: string };
type Stage = {
  work: (save: MediaDraftChannel['save']) => Promise<void>;
  task: Promise<void> | null;
  active: boolean;
  cancelled: boolean;
  wrote: boolean;
  error: Error | null;
};

/** One scope's in-memory projection. Rust owns all durable metadata, bytes and receipts. */
export class MediaDraftChannel {
  readonly scope: MediaDraftScope;
  private state: MediaDraftState = Object.freeze({
    phase: 'loading',
    value: null,
    content: null,
    saving: false,
    sending: false,
    reviewing: false,
    frozen: false,
    error: null,
  });
  private readonly listeners = new Set<() => void>();
  private readonly port: MediaDraftPort;
  private readonly admitted: () => boolean;
  private retirement: 'preserve' | 'discard' | null = null;
  private tail: Promise<void> = Promise.resolve();
  private loading: Promise<void> | null = null;
  private pumping: Promise<void> | null = null;
  private sendTask: Promise<MediaDraftSendResult> | null = null;
  private intent: Intent | null = null;
  private readonly writes: Write[] = [];
  private writeError: Error | null = null;
  private readonly leases = new Set<string>();
  private readonly leaseReleases = new Map<string, Promise<void>>();
  private readonly deferredReleases: (() => Promise<void>)[] = [];
  private deferredDrain: Promise<void> | null = null;
  private readonly stages = new Set<Stage>();
  private retirementTask: Promise<void> | null = null;
  private releasing: Promise<void> | null = null;
  private editing: { token: string; task: Promise<void> } | null = null;
  private clearing = false;
  private clearTask: Promise<MediaDraftDto> | null = null;

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
  }
  getSnapshot = (): MediaDraftState => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(patch: Partial<MediaDraftState>) {
    if (this.retirement === 'discard') return;
    const next = { ...this.state, ...patch };
    this.state = Object.freeze({
      ...next,
      frozen:
        next.phase === 'closed' ||
        next.sending ||
        next.reviewing ||
        this.clearing ||
        !!this.intent ||
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
    if (this.retirement || !this.admitted()) throw new MediaDraftFailure('closed');
  }
  private draining() {
    if (this.retirement === 'discard' || (!this.retirement && !this.admitted()))
      throw new MediaDraftFailure('closed');
  }
  private loaded() {
    if (!this.state.value) throw this.state.error ?? new MediaDraftFailure('notLoaded');
  }
  private enqueue<T>(work: () => Promise<T>): Promise<T> {
    const task = this.tail.then(work);
    this.tail = task.then(
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
    if (this.loading) return this.loading;
    if (this.state.value && !this.intent && !this.state.error) return Promise.resolve();
    const task = this.enqueue(async () => {
      this.draining();
      try {
        const value = await this.port.load(this.scope);
        this.draining();
        this.accept(value, this.writes.length > 0);
        this.publish({ phase: this.retirement ? 'closed' : 'ready' });
      } catch (cause) {
        const error = cause instanceof Error ? cause : new Error('Media draft read failed');
        this.publish({ error });
        throw error;
      }
    });
    this.loading = task;
    const settled = () => {
      if (this.loading === task) this.loading = null;
    };
    void task.then(settled, settled);
    return task;
  }

  save(
    content: MediaDraftContent | null,
    assets: readonly MediaDraftIngest[] = [],
  ): Promise<MediaDraftDto> {
    if (content === null && this.intent) return this.clear();
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
        if (!stage.active || stage.cancelled || !this.stages.has(stage))
          throw new MediaDraftFailure('closed');
        this.draining();
      } else if (clearing) this.draining();
      else this.open();
      this.loaded();
      if (this.clearing && !clearing) throw new MediaDraftFailure('busy');
      if ((this.state.sending || this.editing) && !stage) throw new MediaDraftFailure('busy');
      if (content !== null && (this.intent || this.state.value!.attempted))
        throw new MediaDraftFailure('attempted');
      if (content === null && this.intent && !clearing) throw new MediaDraftFailure('attempted');
      const parsed = contentValue(content);
      const known = new Set([
        ...(this.state.value?.assets.map((asset) => asset.id) ?? []),
        ...this.writes.flatMap((write) => write.refs),
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
      this.writes.push(write);
      if (stage) stage.wrote = true;
      this.publish({ content: parsed.content });
      if (this.writeError && !stage) return Promise.reject(this.writeError);
      return this.pump().then(() => write.result!);
    } catch (cause) {
      return Promise.reject(cause);
    }
  }
  clear(): Promise<MediaDraftDto> {
    if (this.clearTask) return this.clearTask;
    try {
      this.open();
      this.loaded();
    } catch (error) {
      return Promise.reject(error);
    }
    const sending = this.sendTask;
    const original = this.intent ?? (sending ? { revision: this.state.value!.revision } : null);
    this.clearing = true;
    const cancelled = [...this.stages];
    for (const stage of cancelled) stage.cancelled = true;
    this.publish({ error: null });
    const task = (async () => {
      // Cancel retires preparation before waiting. An old prepare callback can
      // finish its native IO, but cannot save behind the explicit clear.
      await this.cancelEdit();
      await Promise.allSettled(cancelled.flatMap((stage) => (stage.task ? [stage.task] : [])));
      for (const stage of cancelled) this.stages.delete(stage);
      // An explicit discard waits an already admitted send; it never retries it.
      if (sending) await sending.catch(() => {});
      const intent = this.intent ?? original;
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
        this.intent = null;
        return value;
      }
      const cleared = await this.saveAdmitted(null, [], undefined, true);
      this.intent = null;
      return cleared;
    })();
    this.clearTask = task;
    const settled = () => {
      this.clearTask = null;
      this.clearing = false;
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
    try {
      this.open();
    } catch (error) {
      return Promise.reject(error);
    }
    if (
      this.clearing ||
      this.state.sending ||
      this.editing ||
      this.intent ||
      this.state.value?.attempted
    )
      return Promise.reject(new MediaDraftFailure('busy'));
    const stage: Stage = {
      work,
      task: null,
      active: false,
      cancelled: false,
      wrote: false,
      error: null,
    };
    this.stages.add(stage);
    return this.startStage(stage);
  }
  private startStage(stage: Stage): Promise<void> {
    if (stage.task) return stage.task;
    stage.active = true;
    stage.wrote = false;
    stage.error = null;
    const task = Promise.resolve().then(() => {
      this.draining();
      return stage.work((content, assets) => this.saveAdmitted(content, assets, stage));
    });
    stage.task = task;
    void task.then(
      () => {
        stage.active = false;
        stage.task = null;
        this.stages.delete(stage);
        this.scheduleRelease();
      },
      (cause: unknown) => {
        stage.active = false;
        stage.task = null;
        if (stage.wrote || stage.cancelled || this.retirement === 'discard')
          this.stages.delete(stage);
        else stage.error = cause instanceof Error ? cause : new Error('Media preparation failed');
        this.publish({ error: stage.error ?? this.state.error });
      },
    );
    return task;
  }
  private async drainStages(retry: boolean) {
    for (const stage of this.stages) {
      if (stage.task) await stage.task;
      else if (stage.error) {
        if (!retry) throw stage.error;
        await this.startStage(stage);
      }
    }
  }

  private async reconcile(write: Write) {
    if (!write.baseline) return;
    // Rust's sealed save receipt authenticates the original revision, normalized
    // content and actual ingress byte hashes. JSON equality cannot attest bytes.
    const remote = saveResultValue(
      await this.port.save(this.scope, write.baseline.revision, write.json, write.assets),
    );
    this.draining();
    write.acceptedRevision = remote.acceptedRevision;
    write.result = this.accept(remote.value, true);
  }
  private pump(): Promise<void> {
    if (this.pumping) return this.pumping;
    const task = this.enqueue(async () => {
      this.draining();
      this.publish({ saving: true });
      try {
        while (this.writes.length) {
          this.draining();
          const write = this.writes[0]!;
          await this.reconcile(write);
          if (!write.result) {
            const baseline = this.state.value!;
            write.baseline = baseline;
            const value = saveResultValue(
              await this.port.save(this.scope, baseline.revision, write.json, write.assets),
            );
            this.draining();
            write.acceptedRevision = value.acceptedRevision;
            write.result = this.accept(value.value, true);
          }
          this.writes.shift();
          if (write.acceptedRevision !== write.result.revision) {
            if (this.writes.length) {
              // An old receipt may return independently newer durable content.
              // Pending local edits were admitted against the older intent.
              this.writes[0]!.baseline = write.baseline;
              throw new MediaDraftFailure('conflict');
            }
          }
          // Rust's accepted revision authenticates this save even when serde
          // normalizes waveform floats. Adopt its canonical projection only when
          // there is no later optimistic local write to preserve.
          if (!this.writes.length) this.publish({ content: dtoValue(write.result).content });
        }
        this.writeError = null;
        this.publish({ error: null });
      } catch (cause) {
        const error = cause instanceof Error ? cause : new Error('Media draft write failed');
        if (this.retirement !== 'discard') this.writeError = error;
        this.publish({ error });
        throw error;
      } finally {
        this.publish({ saving: false });
      }
    });
    this.pumping = task;
    const settled = () => {
      if (this.pumping === task) this.pumping = null;
      this.scheduleRelease();
    };
    void task.then(settled, settled);
    return task;
  }
  private async flushWrites() {
    this.draining();
    if (this.loading) await this.loading;
    this.loaded();
    if (this.pumping) await this.pumping;
    if (this.writes.length) await this.pump();
    this.draining();
    if (this.writeError) throw this.writeError;
  }
  async flush(): Promise<void> {
    if (this.clearTask) await this.clearTask;
    await this.drainStages(false);
    await this.flushWrites();
    if (this.sendTask) await this.sendTask;
    if (this.intent) throw this.state.error ?? new MediaDraftFailure('attempted');
    await this.releaseDeferred();
  }
  async retry(): Promise<void> {
    this.open();
    await this.drainStages(true);
    if (this.intent) {
      await this.send();
      return;
    }
    if (!this.state.value) await this.load();
    await this.flush();
  }

  send(): Promise<MediaDraftSendResult> {
    if (this.sendTask) return this.sendTask;
    try {
      this.open();
      this.loaded();
      if (this.clearing || this.editing) throw new MediaDraftFailure('busy');
    } catch (error) {
      return Promise.reject(error);
    }
    this.publish({ sending: true });
    const task = (async () => {
      try {
        await this.drainStages(false);
        await this.flushWrites();
        this.draining();
        const intent = this.intent ?? { revision: this.state.value!.revision };
        this.intent = intent;
        return await this.enqueue(async () => {
          this.draining();
          const result = await this.port.send(this.scope, intent.revision);
          this.draining();
          if (!Array.isArray(result.entries)) invalid();
          const draft = this.accept(result.draft);
          this.intent = null;
          return Object.freeze({ entries: Object.freeze([...result.entries]), draft });
        });
      } catch (cause) {
        if (
          cause &&
          typeof cause === 'object' &&
          'code' in cause &&
          cause.code === 'ERR_ZOEN_DRAFT_REJECTED'
        )
          this.intent = null;
        const error = cause instanceof Error ? cause : new Error('Media draft send failed');
        this.publish({ error });
        throw error;
      }
    })();
    this.sendTask = task;
    const settled = () => {
      if (this.sendTask === task) this.sendTask = null;
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
        this.editing ||
        this.clearing ||
        this.state.sending ||
        this.stages.size ||
        this.intent ||
        this.state.value?.attempted
      )
        throw new MediaDraftFailure('busy');
      const content = this.state.content;
      if (
        content?.kind !== 'attachments' ||
        !content.items.some((item) => item.asset_id === assetId)
      )
        throw new MediaDraftFailure('invalid');
    } catch (error) {
      return Promise.reject(error);
    }
    const editing = { token: presentationId, task: Promise.resolve() };
    this.editing = editing;
    const task = this.enqueue(async () => {
      this.draining();
      // prepare() flushed all prior writes before the presentation was admitted.
      if (this.writes.length || this.writeError) throw new MediaDraftFailure('busy');
      const baseline = this.state.value!;
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
      if (this.editing === editing) this.editing = null;
      this.publish({ reviewing: false });
      this.scheduleRelease();
    };
    void task.then(settled, (reason: unknown) => {
      settled();
      if (!this.retirement)
        this.publish({
          error: reason instanceof Error ? reason : new Error('Media review failed'),
        });
    });
    return task;
  }
  private async cancelEdit(): Promise<void> {
    const editing = this.editing;
    if (!editing) return;
    await this.port.cancelPreview!(editing.token);
    await editing.task.catch(() => {});
  }
  async materialize(assetId: string): Promise<MaterializedMediaDraftAsset> {
    this.open();
    if (this.clearTask) await this.clearTask;
    await this.flushWrites();
    const value = this.state.value!;
    const expected = value.assets.find((asset) => asset.id === assetId);
    if (!expected) throw new MediaDraftFailure('invalid');
    return this.enqueue(async () => {
      this.draining();
      const result = await this.port.materialize(this.scope, value.revision, assetId);
      try {
        this.open();
        const { uri, ...metadata } = result;
        if (
          !boundedText(uri, 16_384) ||
          !uri ||
          JSON.stringify(assetValue(metadata)) !== JSON.stringify(expected)
        )
          invalid();
        this.leases.add(uri);
        return Object.freeze({ ...expected, uri });
      } catch (error) {
        if (typeof result.uri === 'string') await this.port.release(result.uri);
        throw error;
      }
    });
  }
  release(uri: string): Promise<void> {
    const releasing = this.leaseReleases.get(uri);
    if (releasing) return releasing;
    if (!this.leases.has(uri)) return Promise.resolve();
    const task = Promise.resolve().then(async () => {
      await this.port.release(uri);
      this.leases.delete(uri);
    });
    this.leaseReleases.set(uri, task);
    const settled = () => {
      if (this.leaseReleases.get(uri) === task) this.leaseReleases.delete(uri);
    };
    void task.then(settled, settled);
    return task;
  }
  deferRelease(work: () => Promise<void>): void {
    this.deferredReleases.push(work);
    this.scheduleRelease();
  }
  private scheduleRelease() {
    if (
      this.releasing ||
      !this.deferredReleases.length ||
      this.retirement ||
      this.writeError ||
      [...this.stages].some((stage) => stage.error)
    )
      return;
    const task = Promise.resolve().then(async () => {
      await this.drainStages(false);
      await this.flushWrites();
      if (this.sendTask) await this.sendTask;
      if (!this.intent) await this.releaseDeferred();
    });
    this.releasing = task;
    const settled = () => {
      if (this.releasing === task) this.releasing = null;
    };
    void task.then(settled, (cause: unknown) => {
      settled();
      this.publish({ error: cause instanceof Error ? cause : new Error('Media release failed') });
    });
  }
  private releaseDeferred(): Promise<void> {
    if (this.deferredDrain) return this.deferredDrain;
    const task = Promise.resolve().then(async () => {
      while (this.deferredReleases.length) {
        if (
          this.retirement !== 'discard' &&
          (this.writes.length ||
            this.writeError ||
            this.sendTask ||
            this.intent ||
            this.stages.size ||
            this.clearing)
        )
          return;
        await this.deferredReleases[0]!();
        this.deferredReleases.shift();
      }
    });
    this.deferredDrain = task;
    const settled = () => {
      if (this.deferredDrain === task) this.deferredDrain = null;
    };
    void task.then(settled, settled);
    return task;
  }
  retire(discard: boolean): Promise<void> {
    if (discard) this.retirement = 'discard';
    else if (!this.retirement) this.retirement = 'preserve';
    this.state = Object.freeze({
      ...this.state,
      phase: 'closed',
      frozen: true,
      ...(discard
        ? { value: null, content: null, saving: false, sending: false, error: null }
        : {}),
    });
    this.notify();
    if (discard) {
      this.writes.length = 0;
      this.writeError = null;
      this.intent = null;
    }
    if (this.retirementTask) return this.retirementTask;
    const task = (async () => {
      await this.cancelEdit();
      if (this.retirement === 'discard') {
        await Promise.allSettled(
          [...this.stages].flatMap((stage) => (stage.task ? [stage.task] : [])),
        );
        this.stages.clear();
      }
      if (this.retirement !== 'discard') {
        await this.drainStages(true);
        if (this.state.value || this.loading) await this.flush();
      }
      await this.tail;
      if (this.sendTask)
        await this.sendTask.catch((error: unknown) => {
          if (this.retirement !== 'discard') throw error;
        });
      if (this.retirement === 'discard') {
        this.writes.length = 0;
        this.writeError = null;
        this.intent = null;
      }
      await this.releaseDeferred();
      await Promise.all([...this.leases].map((uri) => this.release(uri)));
      this.state = Object.freeze({
        phase: 'closed',
        value: null,
        content: null,
        saving: false,
        sending: false,
        reviewing: false,
        frozen: true,
        error: null,
      });
      this.notify();
    })();
    this.retirementTask = task;
    const settled = () => {
      if (this.retirementTask === task) this.retirementTask = null;
    };
    void task.then(settled, settled);
    return task;
  }
}
