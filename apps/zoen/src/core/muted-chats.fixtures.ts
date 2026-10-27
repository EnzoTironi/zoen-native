import { MutedChatsSession } from '../notifications/muted-chats.ts';
import type { Snapshot } from './types.ts';
import type { AccountScope } from './account-scope.ts';
import { draftSession, deferred } from './store.fixtures.ts';
export { deferred };
export function mutedSnapshot(): Snapshot {
  const value = draftSession();
  const base = value.conversations[0]!;
  return {
    ...value,
    conversations: ['Alpha', 'Beta'].map((title, index) => ({
      ...base,
      id: `chat-${index}`,
      title,
      muted: true,
      muted_until_ms: '9223372036854775807',
    })),
  };
}
export const scope: AccountScope = {
  identity: 'a'.repeat(64),
  device: 'b'.repeat(64),
  relay: 'https://relay.example',
  epoch: 1,
};
export function mutedFixture() {
  let current: AccountScope | null = { ...scope };
  let snapshot = mutedSnapshot();
  let read = async () => structuredClone(snapshot);
  let write = async (id: string) => {
    snapshot.conversations = snapshot.conversations.map((chat) =>
      chat.id === id ? { ...chat, muted: false, muted_until_ms: null } : chat,
    );
  };
  const calls: Array<{ kind: 'read' | 'write'; id?: string; owner?: AccountScope }> = [];
  const session = new MutedChatsSession({
    scope: () => current,
    read: () => {
      calls.push({ kind: 'read' });
      return read();
    },
    mute: async () => {
      throw new Error('Timed mutation port is not configured in legacy fixture');
    },
    unmute: (id, owner) => {
      calls.push({ kind: 'write', id, owner });
      return write(id);
    },
  });
  session.activate(current);
  return {
    session,
    calls,
    load: () => session.load(session.ticket()),
    select: (id = 'chat-0') => session.select(id, session.ticket()),
    unmute: (confirm: () => Promise<boolean> = async () => true, id = 'chat-0') =>
      session.unmute(id, confirm, session.ticket()),
    scope: (next: AccountScope | null) => {
      current = next;
    },
    snapshot: () => snapshot,
    read: (fn: typeof read) => {
      read = fn;
    },
    write: (fn: typeof write) => {
      write = fn;
    },
    writes: () => calls.filter((call) => call.kind === 'write'),
  };
}
