import { randomUUID } from 'expo-crypto';
import { router } from 'expo-router';

type Launch = Readonly<{ spaceId: string; query: string }>;
const launches = new Map<string, Launch>();

/** Only an opaque handoff enters navigation; private search text stays in memory. */
export function launchConversationAssistant(spaceId: string, query: string) {
  if (!spaceId || !query.trim()) return;
  const ticket = randomUUID();
  while (launches.size >= 4) launches.delete(launches.keys().next().value!);
  launches.set(ticket, { spaceId, query: query.trim() });
  router.push({ pathname: '/ask/[space]', params: { space: spaceId, ticket } });
}
export function readAssistantLaunch(ticket: string | undefined, spaceId: string): Launch | null {
  const launch = ticket ? launches.get(ticket) : undefined;
  return launch?.spaceId === spaceId ? launch : null;
}
export function releaseAssistantLaunch(ticket: string | undefined) {
  if (ticket) launches.delete(ticket);
}
