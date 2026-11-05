import { conversationScope } from './conversation-scope.ts';
import { conversationQueryFits } from './conversation-search.ts';
import type { ClientStore } from './store';
type Launch = Readonly<{ space: string; query: string; scope: string }>;
/** Bounded volatile navigation, not a second search or a history store. */
export class SearchLaunches {
  private readonly values = new Map<string, { value: Launch; unregister(): void }>();
  prepare(ticket: string, space: string, query: string, store: ClientStore): boolean {
    const scope = conversationScope(store.getSnapshot(), store.sessionEpoch, space);
    if (
      !ticket ||
      ticket.length > 128 ||
      !scope ||
      store.isSessionRetiring ||
      !query.trim() ||
      !conversationQueryFits(query) ||
      this.values.has(ticket)
    )
      return false;
    const cleanup = store.registerSessionCleanup(async () => this.release(ticket));
    const unsubscribe = store.subscribe(() => {
      if (
        store.isSessionRetiring ||
        scope !== conversationScope(store.getSnapshot(), store.sessionEpoch, space)
      )
        this.release(ticket);
    });
    const unregister = () => {
      cleanup();
      unsubscribe();
    };
    this.values.set(ticket, { value: Object.freeze({ space, query, scope }), unregister });
    const oldest = this.values.keys().next();
    if (this.values.size > 4 && !oldest.done) this.release(oldest.value);
    return true;
  }
  read(ticket: string | undefined, store: ClientStore): Launch | null {
    const value = ticket ? this.values.get(ticket)?.value : null;
    if (
      !value ||
      store.isSessionRetiring ||
      value.scope !== conversationScope(store.getSnapshot(), store.sessionEpoch, value.space)
    ) {
      if (ticket) this.release(ticket);
      return null;
    }
    return value;
  }
  release(ticket?: string) {
    if (!ticket) return;
    const entry = this.values.get(ticket);
    this.values.delete(ticket);
    entry?.unregister();
  }
}
export const searchLaunches = new SearchLaunches();
