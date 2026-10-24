import type { ClientState } from './store';
import { documentScope } from '../documents/document-authority.ts';
import { homeSpacesFor } from '../home/access.ts';

/** Transient presentation key; canonical membership still controls every read. */
export function conversationScope(state: ClientState, epoch: number, space: string): string | null {
  const owner = documentScope(state, epoch);
  return owner && homeSpacesFor(state).has(space)
    ? JSON.stringify([owner, state.snapshot?.account?.relay_url ?? null, space])
    : null;
}
