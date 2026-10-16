import fixture from './fixtures/native-parity-v1.json' with { type: 'json' };
import { parseItemCard, type Conversation, type Message } from './messages.ts';
import type { NativeRootScreenData } from './native-root-data.ts';
import type { NativeRootPersona, NativeRootRequest } from './native-root-screens.tsx';
import { displayRelativeTime } from './display-clock.ts';
/** Native core DTO -> production browser view data. No alternate screen implementation. */
export function nativeFixtureConversations(): Conversation[] {
  return fixture.spaces.map(space => ({
    id: space.id, title: space.title, kind: space.kind, can_send: true, waiting: null, unreadable: 0,
    unread: space.unread, mentions: fixture.mentions.filter(mention => mention.spaceId === space.id).length, members: space.members.length,
    ...(space.counterpart ? { counterpart: { ...space.counterpart, kind: space.counterpart.kind === 'agent' ? 'agent' as const : 'person' as const } } : {}),
    messages: space.entries.map(entry => ({
      id: entry.id, entry: entry.id, author: entry.author.id, name: entry.author.name,
      text: entry.text, at_ms: entry.atMs, delivery: 'confirmed', failure: null,
      ticks: entry.delivery === 'read' ? 'read' : entry.delivery === 'delivered' ? 'delivered' : 'sent',
      edited: entry.edited, deleted: entry.deleted, forwarded: false, reactions: [], quote: null,
      thread: null, replies: entry.threadReplies, repliers: [], last_reply_ms: null, file: null,
      ...(entry.card ? { card: parseItemCard(entry.card) } : {}),
    } satisfies Message)),
  }));
}
function rootPersona<T extends { kind: string; id: string; name: string }>(persona: T): NativeRootPersona {
  if (persona.kind !== 'person' && persona.kind !== 'agent') throw new Error('Invalid native persona kind.');
  return { ...persona, kind: persona.kind };
}

const capabilityLabels: Record<string, string> = {
  'location.approximate': 'Your approximate location (about 3 km)', location: 'Your precise location',
  'photos.pick': 'Only the photos you pick', 'camera.capture': 'One photo you take now',
  'calendar.freebusy': 'When you’re busy (no event names)', 'calendar.events': 'Your event names and details',
  'calendar.add': 'A new event, after you review it', 'contacts.pick': 'Only the contact you pick (name)',
  'health.steps': 'Your daily step count',
};

/** Genuine Engine DTOs consumed by the same production root screens. No JSX or made-up counts. */
export function nativeFixtureRootData(): NativeRootScreenData {
  const grants: readonly { grantId: string; capability: string; appName: string; spaceTitle: string; always: boolean }[] = fixture.appDeviceGrants;
  return {
    me: rootPersona(fixture.me), contacts: fixture.personas.filter(persona => !persona.isMe).map(rootPersona), syncMode: 'demo',
    requests: fixture.requests.map(request => {
      const status = request.status;
      if (status !== 'pending' && status !== 'stale' && status !== 'approved' && status !== 'denied') throw new Error('Invalid native request status.');
      return { ...request, status, agent: rootPersona(request.agent) } satisfies NativeRootRequest;
    }),
    agents: fixture.agents.map(agent => ({ ...agent, id: agent.persona.id, persona: rootPersona(agent.persona) })),
    mentions: fixture.mentions.map(mention => ({ id: mention.entry.id, spaceId: mention.spaceId, spaceTitle: mention.spaceTitle,
      author: rootPersona(mention.entry.author), text: mention.entry.text, relativeTime: displayRelativeTime(mention.entry.atMs) })),
    items: fixture.items, stats: fixture.stats, account: fixture.account, connection: fixture.connection,
    grants: grants.map(grant => ({ ...grant, capabilityLabel: capabilityLabels[grant.capability]
      ?? (grant.capability.startsWith('net:') ? `Internet access to ${grant.capability.slice(4)}` : grant.capability) })),
  };
}
export { fixture as nativeFixture };
