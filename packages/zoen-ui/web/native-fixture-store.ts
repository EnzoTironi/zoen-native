import fixture from './fixtures/native-parity-v1.json' with { type: 'json' };
import { parseItemCard, type Conversation, type Message } from './messages.ts';
/** Native core DTO -> production browser view data. No alternate screen implementation. */
export function nativeFixtureConversations(): Conversation[] {
  return fixture.spaces.map(space => ({
    id: space.id, title: space.title, kind: space.kind, can_send: true, waiting: null, unreadable: 0,
    unread: space.unread, mentions: 0, members: space.members.length,
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
export { fixture as nativeFixture };
