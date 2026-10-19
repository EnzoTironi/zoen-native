import { MAX_FILE_CAPTION_BYTES, MAX_SELECTED_BYTES } from './types.ts';

export const MAX_ATTACHMENT_ITEMS = 10;
export const MAX_ATTACHMENT_TOTAL_BYTES = MAX_SELECTED_BYTES;

export type AttachmentDraftScope = Readonly<{
  mode: 'account' | 'demo';
  identityId: string;
  deviceId: string;
  spaceId: string;
  replyTo: string | null;
  thread: boolean;
}>;
/** The picker owner verifies the staged file's actual size before admitting it. */
export type AttachmentAsset = Readonly<{
  uri: string;
  name: string;
  mime: string;
  size: number;
}>;
export type AttachmentReceipt = Readonly<{ itemId: string; messageId: string }>;
export type AttachmentDraftItem = AttachmentAsset &
  Readonly<{
    mutationId: string;
    caption: string;
    attempted: boolean;
    accepted: AttachmentReceipt | null;
  }>;
export type AttachmentDraftFailureKind =
  | 'unavailable'
  | 'busy'
  | 'invalid'
  | 'limit'
  | 'size'
  | 'caption'
  | 'pick'
  | 'send'
  | 'cleanup';
export class AttachmentDraftFailure extends Error {
  readonly kind: AttachmentDraftFailureKind;
  constructor(kind: AttachmentDraftFailureKind, cause?: unknown) {
    super(kind, { cause });
    this.name = 'AttachmentDraftFailure';
    this.kind = kind;
  }
}
export type AttachmentDraftState = Readonly<{
  scope: AttachmentDraftScope;
  phase: 'idle' | 'picking' | 'ready' | 'sending' | 'closing' | 'closed';
  items: readonly AttachmentDraftItem[];
  targetFrozen: boolean;
  error: AttachmentDraftFailure | null;
}>;
export type AttachmentBatchResult = Readonly<{
  /** Only receipts newly resolved by this call, including an idempotent retry. */
  accepted: readonly AttachmentReceipt[];
  complete: boolean;
  current: boolean;
}>;
export type AttachmentDraftPort = {
  /** Live route/account/device/access admission; reply selection is owned by this draft. */
  current(scope: AttachmentDraftScope): boolean;
  mutationId(): string;
  send(scope: AttachmentDraftScope, item: AttachmentDraftItem): Promise<AttachmentReceipt>;
  /** Accepts only owned staged URIs; the platform retains its path allowlist. */
  discard(uri: string): void | Promise<void>;
};

function fitsUTF8(value: string, limit: number) {
  let bytes = 0;
  for (const character of value) {
    const code = character.codePointAt(0)!;
    bytes += code <= 0x7f ? 1 : code <= 0x7ff ? 2 : code <= 0xffff ? 3 : 4;
    if (bytes > limit) return false;
  }
  return true;
}
const captionFits = (value: string) =>
  !value.includes('\0') && fitsUTF8(value, MAX_FILE_CAPTION_BYTES);
function hasControl(value: string) {
  return Array.from(value).some((character) => {
    const code = character.codePointAt(0)!;
    return code < 32 || code === 127;
  });
}
const validScopeId = (value: string) =>
  value.trim().length > 0 && fitsUTF8(value, 256) && !hasControl(value);
function validAsset(asset: AttachmentAsset) {
  return (
    typeof asset.uri === 'string' &&
    asset.uri.length > 0 &&
    fitsUTF8(asset.uri, 16_384) &&
    !asset.uri.includes('\0') &&
    typeof asset.name === 'string' &&
    asset.name.trim().length > 0 &&
    fitsUTF8(asset.name, 1024) &&
    !asset.name.includes('/') &&
    !asset.name.includes('\\') &&
    !asset.name.includes('\0') &&
    typeof asset.mime === 'string' &&
    fitsUTF8(asset.mime, 255) &&
    asset.mime.includes('/') &&
    !hasControl(asset.mime) &&
    Number.isSafeInteger(asset.size) &&
    asset.size >= 0
  );
}
const failure = (kind: AttachmentDraftFailureKind, cause: unknown) =>
  cause instanceof AttachmentDraftFailure ? cause : new AttachmentDraftFailure(kind, cause);

/** One ephemeral batch; canonical messages and idempotency receipts stay in Rust. */
export class AttachmentDraft {
  private state: AttachmentDraftState;
  private readonly listeners = new Set<() => void>();
  private readonly pending = new Set<Promise<unknown>>();
  private readonly owned = new Set<string>();
  private readonly cleaning = new Map<string, Promise<void>>();
  private accepting = true;
  private closing: Promise<void> | null = null;
  private epoch = 0;
  private readonly port: AttachmentDraftPort;

  constructor(scope: AttachmentDraftScope, port: AttachmentDraftPort) {
    if (
      !validScopeId(scope.identityId) ||
      !validScopeId(scope.deviceId) ||
      !validScopeId(scope.spaceId) ||
      (scope.replyTo !== null && !validScopeId(scope.replyTo)) ||
      (scope.thread && !scope.replyTo)
    )
      throw new AttachmentDraftFailure('invalid');
    this.port = port;
    this.state = Object.freeze({
      scope: Object.freeze({ ...scope }),
      phase: 'idle',
      items: Object.freeze([]),
      targetFrozen: false,
      error: null,
    });
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private current(epoch = this.epoch) {
    return this.accepting && epoch === this.epoch && this.port.current(this.state.scope);
  }
  private publish(patch: Partial<AttachmentDraftState>, notify = this.current()) {
    this.state = Object.freeze({ ...this.state, ...patch });
    // Accepted outcomes still belong to this batch after navigation, but old
    // asynchronous callbacks never publish into another visible owner.
    if (notify) this.listeners.forEach((listener) => listener());
  }
  private track<T>(task: Promise<T>) {
    this.pending.add(task);
    void task.finally(() => this.pending.delete(task)).catch(() => {});
    return task;
  }
  private cleanup(uri: string): Promise<void> {
    const existing = this.cleaning.get(uri);
    if (existing) return existing;
    if (!this.owned.has(uri)) return Promise.resolve();
    const task = Promise.resolve()
      .then(() => this.port.discard(uri))
      .then(() => {
        this.owned.delete(uri);
      });
    this.cleaning.set(uri, task);
    void task.finally(() => this.cleaning.delete(uri)).catch(() => {});
    return task;
  }
  private async cleanupAll(uris: readonly string[]) {
    const results = await Promise.allSettled(uris.map((uri) => this.cleanup(uri)));
    const rejected = results.find((result) => result.status === 'rejected');
    if (rejected?.status === 'rejected') throw failure('cleanup', rejected.reason);
  }
  retarget(replyTo: string | null, thread: boolean): boolean {
    if (
      !this.current() ||
      this.state.targetFrozen ||
      this.state.phase === 'sending' ||
      (replyTo !== null && !validScopeId(replyTo)) ||
      (thread && !replyTo)
    )
      return false;
    if (replyTo === this.state.scope.replyTo && thread === this.state.scope.thread) return true;
    this.publish({ scope: Object.freeze({ ...this.state.scope, replyTo, thread }) });
    return true;
  }
  pick(work: () => Promise<readonly AttachmentAsset[]>): Promise<void> {
    if (!this.current()) return Promise.reject(new AttachmentDraftFailure('unavailable'));
    if (this.state.phase === 'picking' || this.state.phase === 'sending')
      return Promise.reject(new AttachmentDraftFailure('busy'));
    const visible = new Set(this.state.items.map((item) => item.uri));
    const debt = [...this.owned].filter((uri) => !visible.has(uri));
    if (debt.length)
      return Promise.reject(
        new AttachmentDraftFailure(debt.some((uri) => this.cleaning.has(uri)) ? 'busy' : 'cleanup'),
      );
    const epoch = this.epoch;
    this.publish({ phase: 'picking', error: null });
    return this.track(
      Promise.resolve().then(async () => {
        const added = new Set<string>();
        try {
          if (!this.current(epoch)) return;
          const assets = (await work()).map((asset) => ({ ...asset }));
          for (const asset of assets) {
            if (typeof asset.uri === 'string' && asset.uri && !this.owned.has(asset.uri)) {
              this.owned.add(asset.uri);
              added.add(asset.uri);
            }
          }
          if (!this.current(epoch)) {
            await this.cleanupAll([...added]);
            return;
          }
          const uris = new Set(this.state.items.map((item) => item.uri));
          let total = this.state.items.reduce((bytes, item) => bytes + item.size, 0);
          if (this.state.items.length + assets.length > MAX_ATTACHMENT_ITEMS)
            throw new AttachmentDraftFailure('limit');
          for (const asset of assets) {
            if (!validAsset(asset) || uris.has(asset.uri))
              throw new AttachmentDraftFailure('invalid');
            uris.add(asset.uri);
            total += asset.size;
            if (total > MAX_ATTACHMENT_TOTAL_BYTES) throw new AttachmentDraftFailure('size');
          }
          const ids = new Set(this.state.items.map((item) => item.mutationId));
          const items = assets.map((asset) => {
            const mutationId = this.port.mutationId();
            if (!/^[A-Za-z0-9_-]{1,128}$/.test(mutationId) || ids.has(mutationId))
              throw new AttachmentDraftFailure('invalid');
            ids.add(mutationId);
            return Object.freeze({
              ...asset,
              mutationId,
              caption: '',
              attempted: false,
              accepted: null,
            });
          });
          this.publish({ items: Object.freeze([...this.state.items, ...items]) });
        } catch (cause) {
          let error = failure('pick', cause);
          try {
            await this.cleanupAll([...added]);
          } catch (cause) {
            error = failure('cleanup', cause);
          }
          if (this.current(epoch)) {
            this.publish({ error });
            throw error;
          }
        } finally {
          if (this.accepting && epoch === this.epoch)
            this.publish({ phase: this.state.items.length ? 'ready' : 'idle' });
        }
      }),
    );
  }
  setCaption(mutationId: string, caption: string): boolean {
    if (!this.current() || this.state.phase === 'sending' || this.state.phase === 'picking')
      return false;
    const item = this.state.items.find((item) => item.mutationId === mutationId);
    if (!item || item.attempted) return false;
    if (!captionFits(caption)) {
      this.publish({ error: new AttachmentDraftFailure('caption') });
      return false;
    }
    if (item.caption === caption) return true;
    this.publish({
      items: Object.freeze(
        this.state.items.map((value) =>
          value === item ? Object.freeze({ ...value, caption }) : value,
        ),
      ),
      error: null,
    });
    return true;
  }
  remove(mutationId: string): Promise<void> {
    if (!this.current()) return Promise.reject(new AttachmentDraftFailure('unavailable'));
    if (this.state.phase === 'sending' || this.state.phase === 'picking')
      return Promise.reject(new AttachmentDraftFailure('busy'));
    const item = this.state.items.find((item) => item.mutationId === mutationId);
    if (!item || (item.attempted && !item.accepted)) return Promise.resolve();
    const items = Object.freeze(this.state.items.filter((value) => value !== item));
    this.publish({ items, phase: items.length ? 'ready' : 'idle', error: null });
    return this.track(
      this.cleanup(item.uri).catch((cause: unknown) => {
        const error = failure('cleanup', cause);
        this.publish({ error });
        throw error;
      }),
    );
  }
  send(): Promise<AttachmentBatchResult> {
    if (!this.current() || this.state.phase === 'sending' || this.state.phase === 'picking')
      return Promise.resolve({ accepted: [], complete: false, current: this.current() });
    const epoch = this.epoch;
    const scope = this.state.scope;
    const batch = this.state.items;
    const accepted: AttachmentReceipt[] = [];
    this.publish({
      phase: 'sending',
      error: null,
      targetFrozen: batch.length > 0 || this.state.targetFrozen,
    });
    return this.track(
      Promise.resolve().then(async () => {
        let cleanupError: AttachmentDraftFailure | null = null;
        try {
          for (const original of batch) {
            if (!this.current(epoch)) break;
            let item = this.state.items.find((item) => item.mutationId === original.mutationId)!;
            if (!item.accepted) {
              item = Object.freeze({ ...item, attempted: true });
              this.publish({
                items: Object.freeze(
                  this.state.items.map((value) =>
                    value.mutationId === item.mutationId ? item : value,
                  ),
                ),
              });
              if (!this.current(epoch)) break;
              // After the first attempt, bytes, caption and target stay immutable:
              // a lost response can still have committed this exact core receipt.
              const result = await this.port.send(scope, item);
              const receipt = Object.freeze({ ...result });
              accepted.push(receipt);
              this.publish({
                items: Object.freeze(
                  this.state.items.map((value) =>
                    value.mutationId === item.mutationId
                      ? Object.freeze({ ...value, accepted: receipt })
                      : value,
                  ),
                ),
              });
            }
            try {
              await this.cleanup(item.uri);
            } catch (cause) {
              cleanupError ??= failure('cleanup', cause);
            }
          }
          if (this.current(epoch) && cleanupError) this.publish({ error: cleanupError });
        } catch (cause) {
          if (this.current(epoch)) this.publish({ error: failure('send', cause) });
        } finally {
          if (this.accepting && epoch === this.epoch)
            this.publish({ phase: this.state.items.length ? 'ready' : 'idle' });
        }
        return Object.freeze({
          accepted: Object.freeze(accepted),
          complete: this.state.items.length > 0 && this.state.items.every((item) => item.accepted),
          current: this.current(epoch),
        });
      }),
    );
  }
  close = (): Promise<void> => {
    if (this.state.phase === 'closed') return Promise.resolve();
    if (this.closing) return this.closing;
    const notify = this.current();
    this.accepting = false;
    ++this.epoch;
    const task = Promise.resolve().then(async () => {
      while (this.pending.size) await Promise.allSettled(this.pending);
      try {
        await this.cleanupAll([...this.owned]);
      } catch (cause) {
        const error = failure('cleanup', cause);
        this.publish({ error });
        throw error;
      }
      this.publish({ items: Object.freeze([]), phase: 'closed', error: null });
    });
    this.closing = task;
    void task.finally(() => (this.closing = null)).catch(() => {});
    // Publish only after the fence and coalesced drain exist: a subscriber may
    // synchronously request close again before React commits its next render.
    this.publish({ phase: 'closing' }, notify);
    return task;
  };
}
