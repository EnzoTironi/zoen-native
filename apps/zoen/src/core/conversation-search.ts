import type { MessageSearchPage } from './generated/MessageSearchPage';
import type { UniversalHit } from './generated/UniversalHit';

export type ConversationSearchState = Readonly<{
  active: boolean;
  query: string;
  hits: readonly UniversalHit[];
  index: number;
  cursor: string | null;
  loading: boolean;
  completed: boolean;
  error: string | null;
}>;
export type ConversationSearchSource = {
  current(): boolean;
  fitsQuery?(query: string): boolean;
  search(query: string, cursor: string | null): Promise<MessageSearchPage>;
};
const initial = (): ConversationSearchState => ({
  active: false,
  query: '',
  hits: [],
  index: -1,
  cursor: null,
  loading: false,
  completed: false,
  error: null,
});

/** The canonical search limit is UTF-8 bytes, rather than keyboard characters. */
export function conversationQueryFits(query: string): boolean {
  let bytes = 0;
  for (const character of query) {
    const code = character.codePointAt(0)!;
    bytes += code <= 0x7f ? 1 : code <= 0x7ff ? 2 : code <= 0xffff ? 3 : 4;
    if (bytes > 512) return false;
  }
  return true;
}

/** Transient search only. Generation and owner checks fence every asynchronous read. */
export class ConversationSearch {
  private state = initial();
  private generation = 0;
  private firstPage: string | null = null;
  private dirty = false;
  private listeners = new Set<() => void>();
  private readonly source: ConversationSearchSource;
  constructor(source: ConversationSearchSource, query = '') {
    this.state = { ...initial(), query };
    this.source = source;
  }
  isCurrent() {
    return this.state.active && this.source.current();
  }
  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
  private publish(change: Partial<ConversationSearchState>) {
    this.state = { ...this.state, ...change };
    for (const listener of this.listeners) listener();
  }
  open() {
    this.publish({ active: true });
    if (this.dirty) void this.refreshCanonical();
  }
  /** Keep the reviewed projection when returning, but retire all pending reads. */
  suspend() {
    this.generation++;
    this.publish({ active: false, loading: false });
  }
  close() {
    this.generation++;
    this.firstPage = null;
    this.dirty = false;
    this.state = initial();
    for (const listener of this.listeners) listener();
  }
  setQuery(query: string) {
    this.generation++;
    this.firstPage = null;
    this.dirty = false;
    this.publish({
      query,
      hits: [],
      index: -1,
      cursor: null,
      loading: false,
      completed: false,
      error: null,
    });
  }
  fitsQuery(query: string) {
    return this.source.fitsQuery?.(query) ?? conversationQueryFits(query);
  }
  async search() {
    const query = this.state.query.trim();
    const generation = ++this.generation;
    if (!this.state.active || !query || !this.fitsQuery(query) || !this.source.current()) return;
    this.dirty = false;
    this.firstPage = null;
    this.publish({
      loading: true,
      completed: false,
      error: null,
      hits: [],
      index: -1,
      cursor: null,
    });
    try {
      const page = await this.source.search(query, null);
      if (!this.valid(generation)) return;
      const hits = uniqueHits(page.hits);
      this.firstPage = pageKey(page);
      this.publish({
        hits,
        cursor: page.next_cursor,
        index: hits.length ? 0 : -1,
        loading: false,
        completed: true,
      });
    } catch (reason) {
      if (this.valid(generation)) this.publish({ loading: false, error: failure(reason) });
    }
  }
  /** Requalify against Core after a canonical event, including events received offscreen.
   * Cursor bytes are compared opaquely: an unchanged clock/read preserves accepted pages;
   * a changed index or first-page presentation replaces them from the fresh page. */
  async refreshCanonical() {
    this.dirty = true;
    const generation = ++this.generation;
    if (!this.isCurrent()) return;
    const query = this.state.query.trim();
    if (!query || !this.fitsQuery(query)) return;
    this.publish({ loading: true, error: null });
    try {
      const page = await this.source.search(query, null);
      if (!this.valid(generation)) return;
      const key = pageKey(page),
        unchanged = this.firstPage !== null && key === this.firstPage;
      this.firstPage = key;
      this.dirty = false;
      this.publish({
        ...(unchanged
          ? {}
          : {
              hits: uniqueHits(page.hits),
              cursor: page.next_cursor,
              index: page.hits.length ? 0 : -1,
            }),
        loading: false,
        completed: true,
      });
    } catch (reason) {
      if (this.valid(generation)) {
        this.firstPage = null;
        this.publish({
          hits: [],
          cursor: null,
          index: -1,
          loading: false,
          completed: false,
          error: failure(reason),
        });
      }
    }
  }
  /** Search pages are newest first; older advances, newer moves back. */
  async move(direction: 'older' | 'newer') {
    if (!this.isCurrent() || this.state.loading) return;
    const next = this.state.index + (direction === 'older' ? 1 : -1);
    if (next < 0) return;
    if (next < this.state.hits.length) {
      this.publish({ index: next });
      return;
    }
    if (direction === 'older') await this.append(next);
  }
  /** List consumers load the next canonical page without moving their selection. */
  async loadMore(observed: ConversationSearchState = this.state) {
    if (observed !== this.state) return;
    await this.append(this.state.index);
  }
  private async append(next: number) {
    const cursor = this.state.cursor;
    if (!this.isCurrent() || this.state.loading || !cursor) return;
    const generation = ++this.generation;
    const query = this.state.query.trim();
    this.publish({ loading: true, error: null });
    try {
      const page = await this.source.search(query, cursor);
      if (!this.valid(generation)) return;
      const hits = uniqueHits([...this.state.hits, ...page.hits]);
      if (
        page.next_cursor === cursor ||
        (hits.length === this.state.hits.length && page.next_cursor)
      ) {
        throw new Error('Search did not advance.');
      }
      this.publish({
        hits,
        cursor: page.next_cursor,
        index: next < hits.length ? next : this.state.index,
        loading: false,
      });
    } catch (reason) {
      // A stale canonical cursor must be restarted explicitly, never silently skip a page.
      if (this.valid(generation))
        this.publish({ loading: false, error: failure(reason), completed: false });
    }
  }
  private valid(generation: number) {
    return generation === this.generation && this.state.active && this.source.current();
  }
}
function uniqueHits(hits: readonly UniversalHit[]): UniversalHit[] {
  const seen = new Set<string>();
  return hits.filter((hit) => {
    const key = JSON.stringify([hit.kind, hit.space_id, hit.ref_id]);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
function failure(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

function pageKey(page: MessageSearchPage): string {
  return JSON.stringify([page.hits, page.next_cursor]);
}
