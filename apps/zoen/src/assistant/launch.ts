import { randomUUID } from 'expo-crypto';
import { router } from 'expo-router';
import type { ClientStore } from '../core/store';
import { conversationScope } from '../core/conversation-scope';

type Launch = Readonly<{ spaceId: string; query: string; scope: string }>;
const launches = new Map<string, Launch>();

/** Only an opaque handoff enters navigation; private search text stays in memory. */
export function launchConversationAssistant(spaceId: string, query: string, store: ClientStore) {
  const scope = conversationScope(store.getSnapshot(), store.sessionEpoch, spaceId);
  if (!scope || store.isSessionRetiring || !query.trim()) return;
  const ticket = randomUUID();
  while (launches.size >= 4) launches.delete(launches.keys().next().value!);
  launches.set(ticket, { spaceId, query: query.trim(), scope });
  router.push({ pathname: '/ask/[space]', params: { space: spaceId, ticket } });
}
export function readAssistantLaunch(
  ticket: string | undefined,
  spaceId: string,
  store: ClientStore,
): Launch | null {
  const launch = ticket ? launches.get(ticket) : undefined;
  return !store.isSessionRetiring &&
    launch?.spaceId === spaceId &&
    launch.scope === conversationScope(store.getSnapshot(), store.sessionEpoch, spaceId)
    ? launch
    : null;
}
export function releaseAssistantLaunch(ticket: string | undefined) {
  if (ticket) launches.delete(ticket);
}
