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
  private listeners = new Set<() => void>();
  private readonly source: ConversationSearchSource;
  constructor(source: ConversationSearchSource) {
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
  }
  close() {
    this.generation++;
    this.state = initial();
    for (const listener of this.listeners) listener();
  }
  setQuery(query: string) {
    this.generation++;
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
  async search() {
    const query = this.state.query.trim();
    const generation = ++this.generation;
    if (!this.state.active || !query || !conversationQueryFits(query) || !this.source.current())
      return;
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
  /** Search pages are newest first; older advances, newer moves back. */
  async move(direction: 'older' | 'newer') {
    if (!this.source.current() || this.state.loading) return;
    const next = this.state.index + (direction === 'older' ? 1 : -1);
    if (next < 0) return;
    if (next < this.state.hits.length) {
      this.publish({ index: next });
      return;
    }
    const cursor = this.state.cursor;
    if (!cursor || direction !== 'older') return;
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
    if (seen.has(hit.ref_id)) return false;
    seen.add(hit.ref_id);
    return true;
  });
}
function failure(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}
