import type { SharedReplyDraft } from './generated/SharedReplyDraft';
import type { ReplySelection } from './generated/ReplySelection';
import type { TimelineEntry } from './types';

export type NormalDraftScope = Readonly<{
  owner: Readonly<{
    identityId: string;
    deviceId: string;
    sessionEpoch: number;
    mode: 'account' | 'demo';
  }>;
  spaceId: string;
  threadRootId: string | null;
}>;
export type NormalDraftSnapshot = Readonly<{
  revision: string;
  text: string;
  reply_to: string | null;
  reply_quote?: ReplySelection | null;
  shared_reply?: SharedReplyDraft | null;
}>;
export type NormalDraftPort = {
  load(scope: NormalDraftScope): Promise<NormalDraftSnapshot>;
  save(
    scope: NormalDraftScope,
    expectedRevision: string,
    text: string,
    replyTo: string | null,
    replyQuote?: ReplySelection | null,
    sharedReply?: SharedReplyDraft | null,
  ): Promise<NormalDraftSnapshot>;
  send(
    scope: NormalDraftScope,
    expectedRevision: string,
  ): Promise<{ entry: TimelineEntry; draft: NormalDraftSnapshot }>;
};
export class NormalDraftFailure extends Error {
  readonly kind: 'closed' | 'notLoaded' | 'empty' | 'load' | 'save' | 'send' | 'conflict';
  constructor(kind: NormalDraftFailure['kind'], cause?: unknown) {
    super(cause instanceof Error ? cause.message : `Message draft ${kind}`, { cause });
    this.name = 'NormalDraftFailure';
    this.kind = kind;
  }
}
export type NormalDraftState = Readonly<{
  scope: NormalDraftScope;
  text: string;
  replyTo: string | null;
  replyQuote: ReplySelection | null;
  sharedReply: SharedReplyDraft | null;
  textRevision: number;
  replyRevision: number;
  persistedRevision: string;
  loaded: boolean;
  loading: boolean;
  saving: boolean;
  sending: boolean;
  error: NormalDraftFailure | null;
}>;
type Content = Pick<NormalDraftSnapshot, 'text' | 'reply_to' | 'reply_quote' | 'shared_reply'>;
type SendIntent = Content & {
  textRevision: number;
  replyRevision: number;
  persistedRevision: string | null;
  work: Promise<TimelineEntry> | null;
};
type DraftRecord = {
  state: NormalDraftState;
  persisted: NormalDraftSnapshot;
  load: Promise<void> | null;
  autosave: Promise<void> | null;
  failedSave: { baseline: NormalDraftSnapshot; content: Content } | null;
  intent: SendIntent | null;
};
const emptySnapshot = (): NormalDraftSnapshot => ({ revision: '0', text: '', reply_to: null });
const sameContent = (a: Content, b: Content) =>
  a.text === b.text &&
  a.reply_to === b.reply_to &&
  JSON.stringify(a.reply_quote ?? null) === JSON.stringify(b.reply_quote ?? null) &&
  JSON.stringify(a.shared_reply ?? null) === JSON.stringify(b.shared_reply ?? null);
const content = (state: NormalDraftState): Content => ({
  text: state.text,
  reply_to: state.replyTo,
  ...(state.replyQuote ? { reply_quote: state.replyQuote } : {}),
  ...(state.sharedReply ? { shared_reply: state.sharedReply } : {}),
});
const keyFor = (scope: NormalDraftScope) =>
  JSON.stringify([
    scope.owner.mode,
    scope.owner.identityId,
    scope.owner.deviceId,
    scope.owner.sessionEpoch,
    scope.spaceId,
    scope.threadRootId,
  ]);
function checked(value: NormalDraftSnapshot): NormalDraftSnapshot {
  if (
    typeof value.revision !== 'string' ||
    !value.revision.length ||
    value.revision.length > 256 ||
    typeof value.text !== 'string' ||
    (value.reply_to !== null && typeof value.reply_to !== 'string')
  )
    throw new Error('Invalid message draft response');
  return {
    revision: value.revision,
    text: value.text,
    reply_to: value.reply_to,
    ...(value.reply_quote ? { reply_quote: value.reply_quote } : {}),
    ...(value.shared_reply ? { shared_reply: value.shared_reply } : {}),
  };
}

/** Session-owned, memory-only projection. The native port owns durable ciphertext.
 * Retiring this instance is permanent; a new session receives a new controller. */
export class NormalMessageDrafts {
  private readonly records = new Map<string, DraftRecord>();
  private readonly listeners = new Set<() => void>();
  private queue: Promise<void> = Promise.resolve();
  private retirement: 'preserve' | 'discard' | null = null;
  private retireTask: Promise<void> | null = null;
  private readonly port: NormalDraftPort;
  constructor(port: NormalDraftPort) {
    this.port = port;
  }

  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  getSnapshot = (scope: NormalDraftScope): NormalDraftState => this.record(scope).state;

  private record(scope: NormalDraftScope): DraftRecord {
    const key = keyFor(scope);
    const previous = this.records.get(key);
    if (previous) return previous;
    const captured = Object.freeze({ ...scope, owner: Object.freeze({ ...scope.owner }) });
    const value: DraftRecord = {
      state: {
        scope: captured,
        text: '',
        replyTo: null,
        replyQuote: null,
        sharedReply: null,
        textRevision: 0,
        replyRevision: 0,
        persistedRevision: '0',
        loaded: false,
        loading: false,
        saving: false,
        sending: false,
        error: null,
      },
      persisted: emptySnapshot(),
      load: null,
      autosave: null,
      failedSave: null,
      intent: null,
    };
    this.records.set(key, value);
    return value;
  }
  private publish(record: DraftRecord, patch: Partial<NormalDraftState>) {
    if (this.retirement === 'discard') return;
    record.state = { ...record.state, ...patch };
    for (const listener of this.listeners) listener();
  }
  private enqueue<T>(work: () => Promise<T>): Promise<T> {
    const task = this.queue.then(work);
    this.queue = task.then(
      () => {},
      () => {},
    );
    return task;
  }
  private requireDraining() {
    if (this.retirement === 'discard') throw new NormalDraftFailure('closed');
  }
  private requireOpen(record: DraftRecord) {
    if (this.retirement) throw new NormalDraftFailure('closed');
    if (!record.state.loaded) throw new NormalDraftFailure('notLoaded');
  }

  activate(scope: NormalDraftScope): Promise<void> {
    if (this.retirement) return Promise.reject(new NormalDraftFailure('closed'));
    const record = this.record(scope);
    if (record.state.loaded) return Promise.resolve();
    if (record.load) return record.load;
    const task = this.enqueue(async () => {
      if (this.retirement) throw new NormalDraftFailure('closed');
      try {
        const value = checked(await this.port.load(record.state.scope));
        this.requireDraining();
        record.persisted = value;
        this.publish(record, {
          loaded: true,
          text: value.text,
          replyTo: value.reply_to,
          replyQuote: value.reply_quote ?? null,
          sharedReply: value.shared_reply ?? null,
          persistedRevision: value.revision,
          error: null,
        });
      } catch (cause) {
        const error =
          cause instanceof NormalDraftFailure ? cause : new NormalDraftFailure('load', cause);
        this.publish(record, { error });
        throw error;
      }
    });
    record.load = task;
    this.publish(record, { loading: true, error: null });
    void task.then(
      () => {
        record.load = null;
        this.publish(record, { loading: false });
      },
      () => {
        record.load = null;
        this.publish(record, { loading: false });
      },
    );
    return task;
  }

  setText(scope: NormalDraftScope, text: string): boolean {
    const record = this.record(scope);
    if (this.retirement || !record.state.loaded) return false;
    this.publish(record, { text, textRevision: record.state.textRevision + 1 });
    this.schedule(record);
    return true;
  }
  isPersisted(scope: NormalDraftScope): boolean {
    const record = this.record(scope);
    return (
      !this.retirement &&
      record.state.loaded &&
      !record.intent &&
      !record.failedSave &&
      sameContent(content(record.state), record.persisted)
    );
  }
  hasPendingSend(scope: NormalDraftScope): boolean {
    return this.record(scope).intent !== null;
  }
  setReply(
    scope: NormalDraftScope,
    replyTo: string | null,
    replyQuote: ReplySelection | null = null,
  ): boolean {
    const record = this.record(scope);
    if (this.retirement || !record.state.loaded) return false;
    if (replyQuote && replyQuote.source_id !== replyTo) return false;
    const frozenQuote = replyQuote
      ? { ...replyQuote, spans: replyQuote.spans.map((span) => ({ ...span })) }
      : null;
    this.publish(record, {
      replyTo,
      replyQuote: frozenQuote,
      sharedReply: null,
      replyRevision: record.state.replyRevision + 1,
    });
    this.schedule(record);
    return true;
  }

  setSharedReply(scope: NormalDraftScope, quote: SharedReplyDraft): boolean {
    const record = this.record(scope);
    if (
      this.retirement ||
      !record.state.loaded ||
      record.intent ||
      scope.threadRootId ||
      quote.space_id === scope.spaceId
    )
      return false;
    this.publish(record, {
      replyTo: null,
      replyQuote: null,
      sharedReply: Object.freeze({
        ...quote,
        ...(quote.selection
          ? {
              selection: {
                ...quote.selection,
                spans: quote.selection.spans.map((span) => ({ ...span })),
              },
            }
          : {}),
      }),
      replyRevision: record.state.replyRevision + 1,
    });
    this.schedule(record);
    return true;
  }

  private schedule(record: DraftRecord) {
    if (
      this.retirement ||
      record.autosave ||
      record.intent ||
      record.failedSave ||
      !record.state.loaded ||
      sameContent(content(record.state), record.persisted)
    )
      return;
    const task = this.enqueue(async () => {
      this.requireDraining();
      // send() establishes its barrier synchronously, including before a queued
      // autosave starts. Later typing must never advance that send's revision.
      if (record.intent || record.failedSave) return;
      await this.persist(record, content(record.state));
    });
    record.autosave = task;
    const settled = () => {
      record.autosave = null;
      this.schedule(record);
    };
    void task.then(settled, settled);
  }

  private async reconcile(record: DraftRecord) {
    const failed = record.failedSave;
    if (!failed) return;
    const remote = checked(await this.port.load(record.state.scope));
    this.requireDraining();
    if (
      !(
        (remote.revision === failed.baseline.revision && sameContent(remote, failed.baseline)) ||
        sameContent(remote, failed.content)
      )
    )
      throw new NormalDraftFailure('conflict');
    record.persisted = remote;
    record.failedSave = null;
    this.publish(record, { persistedRevision: remote.revision });
  }
  private async persist(record: DraftRecord, target: Content) {
    this.requireDraining();
    if (sameContent(target, record.persisted)) return;
    const baseline = record.persisted;
    this.publish(record, { saving: true });
    try {
      const value = checked(
        await this.port.save(
          record.state.scope,
          baseline.revision,
          target.text,
          target.reply_to,
          target.reply_quote ?? null,
          target.shared_reply ?? null,
        ),
      );
      this.requireDraining();
      if (!sameContent(value, target)) throw new Error('Message draft save changed its intent');
      record.persisted = value;
      record.failedSave = null;
      this.publish(record, { persistedRevision: value.revision, error: null });
    } catch (cause) {
      if (this.retirement !== 'discard') record.failedSave = { baseline, content: target };
      const error =
        cause instanceof NormalDraftFailure ? cause : new NormalDraftFailure('save', cause);
      this.publish(record, { error });
      throw error;
    } finally {
      this.publish(record, { saving: false });
    }
  }

  async flush(scope: NormalDraftScope): Promise<void> {
    this.requireDraining();
    const record = this.record(scope);
    if (record.intent) {
      await this.sendIntent(record, record.intent);
      // A successful retry may have uncovered newer, independently typed text.
    }
    if (record.load) await record.load;
    if (!record.state.loaded) throw record.state.error ?? new NormalDraftFailure('notLoaded');
    await this.enqueue(async () => {
      this.requireDraining();
      if (record.intent) return;
      try {
        await this.reconcile(record);
        await this.persist(record, content(record.state));
        this.publish(record, { error: null });
      } catch (cause) {
        const error =
          cause instanceof NormalDraftFailure ? cause : new NormalDraftFailure('save', cause);
        this.publish(record, { error });
        throw error;
      }
    });
  }

  /** Read-only admission barrier. Unlike flush/retry, this NEVER resumes an
   * ambiguous Send. New typing remains owned by its later queued autosave. */
  async withEmptyDraft<T>(
    scope: NormalDraftScope,
    work: (revision: string, current: () => boolean) => Promise<T>,
  ): Promise<T> {
    await this.activate(scope);
    return this.enqueue(async () => {
      const record = this.record(scope);
      this.requireOpen(record);
      if (
        record.intent ||
        record.failedSave ||
        record.state.sending ||
        record.state.saving ||
        record.state.error ||
        record.state.text ||
        record.state.replyTo ||
        record.persisted.text ||
        record.persisted.reply_to
      )
        throw new NormalDraftFailure('conflict');
      const textRevision = record.state.textRevision,
        replyRevision = record.state.replyRevision;
      const revision = record.persisted.revision;
      const current = () =>
        !this.retirement &&
        !record.intent &&
        record.state.textRevision === textRevision &&
        record.state.replyRevision === replyRevision &&
        record.persisted.revision === revision &&
        !record.state.text &&
        !record.state.replyTo;
      return work(revision, current);
    });
  }

  send(scope: NormalDraftScope): Promise<TimelineEntry> {
    const record = this.record(scope);
    try {
      this.requireOpen(record);
      if (record.intent) return this.sendIntent(record, record.intent);
      if (!record.state.text.trim()) throw new NormalDraftFailure('empty');
      const intent: SendIntent = {
        ...content(record.state),
        textRevision: record.state.textRevision,
        replyRevision: record.state.replyRevision,
        persistedRevision: null,
        work: null,
      };
      record.intent = intent;
      return this.sendIntent(record, intent);
    } catch (cause) {
      return Promise.reject(cause);
    }
  }
  private sendIntent(record: DraftRecord, intent: SendIntent): Promise<TimelineEntry> {
    if (intent.work) return intent.work;
    const task = this.enqueue(async () => {
      this.requireDraining();
      try {
        if (intent.persistedRevision === null) {
          await this.reconcile(record);
          await this.persist(record, intent);
          intent.persistedRevision = record.persisted.revision;
        }
        this.requireDraining();
        const result = await this.port.send(record.state.scope, intent.persistedRevision);
        this.requireDraining();
        record.persisted = checked(result.draft);
        record.intent = null;
        this.publish(record, {
          text: record.state.textRevision === intent.textRevision ? '' : record.state.text,
          replyTo:
            record.state.replyRevision === intent.replyRevision ? null : record.state.replyTo,
          replyQuote:
            record.state.replyRevision === intent.replyRevision ? null : record.state.replyQuote,
          sharedReply:
            record.state.replyRevision === intent.replyRevision ? null : record.state.sharedReply,
          persistedRevision: record.persisted.revision,
          error: null,
        });
        return result.entry;
      } catch (cause) {
        const rejected =
          typeof cause === 'object' &&
          cause !== null &&
          'code' in cause &&
          cause.code === 'ERR_ZOEN_DRAFT_REJECTED';
        if (intent.persistedRevision === null || rejected) {
          // This exclusive native code proves receipt absence and exact current
          // revision before rejecting, including after a previously lost reply.
          // No generic validation/transport error provides that guarantee.
          // A lost save response is reconciled separately through failedSave.
          if (record.intent === intent) record.intent = null;
        }
        const error =
          cause instanceof NormalDraftFailure ? cause : new NormalDraftFailure('send', cause);
        this.publish(record, { error });
        throw error;
      }
    });
    intent.work = task;
    this.publish(record, { sending: true, error: null });
    const settled = () => {
      intent.work = null;
      this.publish(record, { sending: Boolean(record.intent?.work) });
      this.schedule(record);
    };
    void task.then(settled, settled);
    return task;
  }

  async retry(scope: NormalDraftScope): Promise<void> {
    if (this.retirement) throw new NormalDraftFailure('closed');
    const record = this.record(scope);
    if (!record.state.loaded) return this.activate(scope);
    await this.flush(scope);
  }

  retire({ flush }: { flush: boolean }): Promise<void> {
    if (!flush) {
      this.retirement = 'discard';
      for (const record of this.records.values()) {
        record.persisted = emptySnapshot();
        record.failedSave = null;
        record.intent = null;
        record.state = {
          ...record.state,
          text: '',
          replyTo: null,
          replyQuote: null,
          sharedReply: null,
          persistedRevision: '0',
          loaded: false,
          loading: false,
          saving: false,
          sending: false,
          error: null,
        };
      }
      for (const listener of this.listeners) listener();
      return this.queue;
    }
    if (this.retirement === 'discard') return this.queue;
    this.retirement = 'preserve';
    if (this.retireTask) return this.retireTask;
    const task = Promise.resolve().then(async () => {
      const results = await Promise.allSettled(
        [...this.records.values()]
          .filter((record) => record.state.loaded)
          .map((record) => this.flush(record.state.scope)),
      );
      await this.queue;
      const failed = results.find((result) => result.status === 'rejected');
      if (failed?.status === 'rejected') throw failed.reason;
    });
    this.retireTask = task;
    void task.then(
      () => {
        this.retireTask = null;
      },
      () => {
        this.retireTask = null;
      },
    );
    return task;
  }
}
