import type { RecentAttachment } from '../core/generated/RecentAttachment';
import type { RecentAttachmentPage } from '../core/generated/RecentAttachmentPage';
import type { RecentAttachmentReference } from '../core/generated/RecentAttachmentReference';
import type { MediaDraftScope } from './media-draft-types';
import type { AttachmentAsset } from './attachment-draft';
import { MAX_SELECTED_BYTES } from './types.ts';

export type RecentAttachmentKind = 'file' | 'audio';
export type RecentAttachmentPort = Readonly<{
  list(
    scope: MediaDraftScope,
    kind: RecentAttachmentKind,
    query: string,
    cursor: string | null,
  ): Promise<RecentAttachmentPage>;
  materialize(
    scope: MediaDraftScope,
    kind: RecentAttachmentKind,
    reference: RecentAttachmentReference,
  ): Promise<AttachmentAsset>;
  preview(
    scope: MediaDraftScope,
    kind: RecentAttachmentKind,
    reference: RecentAttachmentReference,
    signal: AbortSignal,
  ): Promise<void>;
  release(uri: string): Promise<void>;
}>;
export type RecentAttachmentConfiguration = Readonly<{
  scope: MediaDraftScope;
  port: RecentAttachmentPort;
}>;
export type RecentAttachmentState = Readonly<{
  kind: RecentAttachmentKind;
  query: string;
  searching: boolean;
  loading: boolean;
  items: readonly RecentAttachment[];
  selected: readonly RecentAttachment[];
  nextCursor: string | null;
  error: unknown;
}>;
const empty = (): RecentAttachmentState => ({
  kind: 'file',
  query: '',
  searching: false,
  loading: false,
  items: [],
  selected: [],
  nextCursor: null,
  error: null,
});
export const recentAttachmentId = (row: RecentAttachment) =>
  JSON.stringify([row.reference.space_id, row.reference.message_id]);
const sameScope = (a: MediaDraftScope, b: MediaDraftScope) =>
  a.expected_identity_id === b.expected_identity_id &&
  a.expected_device_id === b.expected_device_id &&
  a.space_id === b.space_id &&
  a.thread_root_id === b.thread_root_id;
const selectable = (row: RecentAttachment) =>
  /^\d+$/.test(row.bytes) &&
  Number.isSafeInteger(Number(row.bytes)) &&
  Number(row.bytes) > 0 &&
  Number(row.bytes) <= MAX_SELECTED_BYTES;
// Query results are borrowed values. Detach their signed version/hash before
// publishing them or retaining a selection across asynchronous materialization.
const snapshot = (row: RecentAttachment): RecentAttachment =>
  Object.freeze({ ...row, reference: Object.freeze({ ...row.reference }) });

/** Replaceable authorized query/selection only. The encrypted media journal
 * remains the sole durable owner, and native Save atomically validates sources. */
export class AttachmentRecentSession {
  private state = empty();
  private owner: Readonly<{
    configuration: RecentAttachmentConfiguration;
    current(): boolean;
    remaining: number;
  }> | null = null;
  private epoch = 0;
  private previews = new Set<AbortController>();
  private listeners = new Set<() => void>();
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };
  private publish(patch: Partial<RecentAttachmentState>) {
    this.state = { ...this.state, ...patch };
    this.listeners.forEach((listener) => listener());
  }
  private admitted(owner: NonNullable<typeof this.owner>, epoch?: number) {
    return this.owner === owner && owner.current() && (epoch === undefined || epoch === this.epoch);
  }
  activate(
    configuration: RecentAttachmentConfiguration,
    kind: RecentAttachmentKind,
    remaining: number,
    current: () => boolean,
  ) {
    if (
      this.owner &&
      this.owner.configuration.port === configuration.port &&
      sameScope(this.owner.configuration.scope, configuration.scope) &&
      this.owner.current() &&
      this.state.kind === kind
    )
      return;
    this.retire();
    if (!current() || remaining < 1) return;
    this.owner = {
      configuration: { scope: Object.freeze({ ...configuration.scope }), port: configuration.port },
      current,
      remaining: Math.min(10, remaining),
    };
    this.publish({ ...empty(), kind });
    void this.load(false);
  }
  retire = () => {
    this.owner = null;
    this.epoch++;
    this.previews.forEach((preview) => preview.abort());
    this.previews.clear();
    this.publish(empty());
  };
  search(active: boolean) {
    if (!this.owner || !this.admitted(this.owner) || this.state.selected.length) return;
    this.publish({ searching: active });
    if (!active && this.state.query) {
      this.publish({ query: '', items: [], nextCursor: null });
      void this.load(false);
    }
  }
  query(value: string) {
    if (
      !this.owner ||
      !this.admitted(this.owner) ||
      !this.state.searching ||
      this.state.selected.length ||
      value.length > 512 ||
      value.includes('\0')
    )
      return;
    this.epoch++;
    this.publish({ query: value, items: [], nextCursor: null, error: null });
    void this.load(false);
  }
  async load(more: boolean) {
    const owner = this.owner;
    if (!owner || !this.admitted(owner) || (more && (this.state.loading || !this.state.nextCursor)))
      return;
    if (more && this.state.items.length >= 500) {
      this.publish({ error: new Error('Refine the file search to continue') });
      return;
    }
    const epoch = ++this.epoch,
      cursor = more ? this.state.nextCursor : null,
      query = this.state.query,
      kind = this.state.kind;
    this.publish({ loading: true, error: null });
    try {
      const result = await owner.configuration.port.list(
        owner.configuration.scope,
        kind,
        query,
        cursor,
      );
      if (!this.admitted(owner, epoch)) return;
      if (
        !Array.isArray(result.items) ||
        result.items.length > 50 ||
        new Set(result.items.map(recentAttachmentId)).size !== result.items.length ||
        (result.next_cursor !== null &&
          (typeof result.next_cursor !== 'string' ||
            result.next_cursor.length > 4096 ||
            result.next_cursor === cursor))
      )
        throw new Error('Invalid recent attachment page');
      const retained = more ? this.state.items : [];
      const ids = new Set(retained.map(recentAttachmentId));
      if (
        this.state.selected.some((selected) =>
          result.items.some(
            (row) =>
              recentAttachmentId(row) === recentAttachmentId(selected) &&
              JSON.stringify(row.reference) !== JSON.stringify(selected.reference),
          ),
        )
      )
        throw new Error('The selected file changed');
      const items = [
        ...retained,
        ...result.items.filter((row) => !ids.has(recentAttachmentId(row))).map(snapshot),
      ];
      if (items.length > 500) throw new Error('Refine the file search to continue');
      this.publish({ items, nextCursor: result.next_cursor });
    } catch (error) {
      if (this.admitted(owner, epoch)) this.publish({ error });
    } finally {
      if (this.admitted(owner, epoch)) this.publish({ loading: false });
    }
  }
  find(id: string) {
    return [...this.state.selected, ...this.state.items].find(
      (row) => recentAttachmentId(row) === id,
    );
  }
  select(id: string) {
    const owner = this.owner,
      row = this.find(id);
    if (!owner || !this.admitted(owner) || this.state.searching || !row || !selectable(row)) return;
    const selected = this.state.selected;
    this.publish({
      selected: selected.some((value) => recentAttachmentId(value) === id)
        ? selected.filter((value) => recentAttachmentId(value) !== id)
        : selected.length < owner.remaining &&
            selected.reduce((bytes, item) => bytes + Number(item.bytes), Number(row.bytes)) <=
              MAX_SELECTED_BYTES
          ? [...selected, snapshot(row)]
          : selected,
    });
  }
  async materialize(ids: readonly string[], current: () => boolean) {
    const owner = this.owner;
    if (
      !owner ||
      !this.admitted(owner) ||
      !current() ||
      !ids.length ||
      ids.length > owner.remaining ||
      new Set(ids).size !== ids.length
    )
      throw new Error('Attachment selection changed');
    const rows = ids.map((id) => this.find(id)).map((row) => row && snapshot(row));
    if (
      rows.some((row) => !row || !selectable(row)) ||
      rows.reduce((bytes, row) => bytes + Number(row?.bytes ?? 0), 0) > MAX_SELECTED_BYTES
    )
      throw new Error('Attachment selection unavailable');
    const result: AttachmentAsset[] = [];
    try {
      for (const row of rows as RecentAttachment[]) {
        if (!this.admitted(owner) || !current()) throw new Error('Attachment selection changed');
        const file = await owner.configuration.port.materialize(
          owner.configuration.scope,
          this.state.kind,
          row.reference,
        );
        result.push(file);
        if (
          !this.admitted(owner) ||
          !current() ||
          file.name !== row.name ||
          file.mime !== row.mime ||
          file.size !== Number(row.bytes)
        )
          throw new Error('Attachment selection changed');
      }
      return result;
    } catch (error) {
      await Promise.all(result.map((file) => owner.configuration.port.release(file.uri)));
      throw error;
    }
  }
  async preview(id: string, current: () => boolean) {
    const owner = this.owner,
      row = this.find(id);
    if (!owner || !row || !selectable(row) || !this.admitted(owner) || !current()) return;
    const preview = new AbortController();
    this.previews.add(preview);
    try {
      await owner.configuration.port.preview(
        owner.configuration.scope,
        this.state.kind,
        row.reference,
        preview.signal,
      );
    } finally {
      this.previews.delete(preview);
    }
  }
}
