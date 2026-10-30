import type {
  Conversation,
  Message,
  MessagesState,
} from '../../../../packages/zoen-ui/web/messages.ts';
import { browserMessageEntry } from './browser-message-entry.ts';
import type { Persona } from './generated/Persona';
import type { SpaceSummary } from './generated/SpaceSummary';
import type { Snapshot, TimelineEntry } from './types';
type SnapshotInput = {
  account: { identity: string; device: string; name: string; handle: string; relay: string } | null;
  registered: boolean;
  setupPending: boolean;
  ownName: string | null;
  connection: Extract<MessagesState, { kind: 'active' }>['connection'] | null;
  error: string | null;
  sourceConversations: readonly Conversation[];
  persona(identity: string, name: string): Persona;
  entry(message: Message, conversation: Conversation): TimelineEntry;
};

/** Public display projection only; no vault, worker or session object is accepted. */
export function browserTimelineEntry(
  message: Message,
  conversation: Conversation,
  persona: SnapshotInput['persona'],
): TimelineEntry {
  const quoted = conversation.messages.find((candidate) => candidate.entry === message.quote?.id);
  const root = message.thread
    ? conversation.messages.find((candidate) => candidate.entry === message.thread)
    : undefined;
  return {
    id: message.id,
    ...(message.text_revision ? { text_revision: message.text_revision } : {}),
    seq: message.seq ?? '0',
    author: persona(message.author, message.name),
    at_ms: String(message.at_ms),
    ...browserMessageEntry(message),
    delivery:
      message.delivery === 'sending'
        ? 'Sending'
        : message.delivery === 'failed'
          ? 'Failed'
          : message.ticks === 'read'
            ? 'Read'
            : message.ticks === 'delivered'
              ? 'Delivered'
              : 'Sent',
    reply_to: message.quote
      ? {
          id: quoted?.id ?? message.quote.id,
          author: persona(quoted?.author ?? '', message.quote.name),
          text: message.quote.text,
        }
      : null,
    in_thread: root?.id ?? message.thread,
    thread_replies: message.replies,
    edited: message.edited,
    deleted: message.deleted,
    forwarded: message.forwarded,
    reactions: message.reactions.map((reaction) => ({
      emoji: reaction.emoji,
      mine: reaction.mine,
      people: reaction.participants.map((person) => persona(person.identity, person.name)),
    })),
  };
}

export function browserSnapshot({
  account,
  registered,
  setupPending,
  ownName,
  connection,
  error,
  sourceConversations,
  persona,
  entry,
}: SnapshotInput): Snapshot {
  const conversations: SpaceSummary[] = sourceConversations.map((conversation) => {
    const last = conversation.messages.at(-1);
    return {
      id: conversation.id,
      title: conversation.title,
      kind:
        conversation.kind === 'group'
          ? 'Group'
          : conversation.kind === 'community'
            ? 'Community'
            : 'Direct',
      privacy: conversation.privacy,
      can_write: conversation.can_send && conversation.integrity_error === null,
      members: conversation.roster.map((member) =>
        persona(member.id, member.id === account?.identity ? account.name : 'Desconhecido'),
      ),
      counterpart:
        conversation.kind === 'direct'
          ? (conversation.roster
              .filter((member) => member.id !== account?.identity)
              .map((member) => persona(member.id, 'Desconhecido'))[0] ?? null)
          : null,
      last_preview: last?.deleted ? 'Mensagem apagada' : (last?.text ?? ''),
      last_preview_is_markdown: !!last && !last.deleted && last.text.length > 0,
      last_author: last ? persona(last.author, last.name) : null,
      last_at_ms: String(last?.at_ms ?? 0),
      unread: conversation.unread,
      last_delivery: last ? entry(last, conversation).delivery : null,
      pinned: conversation.prefs.pinned,
      archived: conversation.prefs.archived,
      muted:
        conversation.prefs.muted_until_ms !== null &&
        BigInt(conversation.prefs.muted_until_ms) > BigInt(Date.now()),
      muted_until_ms: conversation.prefs.muted_until_ms,
      pending_requests: 0,
      event_count: conversation.event_count,
    };
  });
  return {
    account: account
      ? {
          identity_id: account.identity,
          device_id: account.device,
          name: ownName ?? account.name,
          handle: account.handle,
          relay_url: account.relay,
          registered: registered,
          unlocked: true,
          setup_pending: setupPending,
        }
      : null,
    me: account ? { ...persona(account.identity, account.name), handle: account.handle } : null,
    connection: {
      state:
        connection?.kind === 'ready'
          ? 'online'
          : connection?.kind === 'failed' || connection?.kind === 'stopped'
            ? 'offline'
            : 'connecting',
      synced: connection?.kind === 'ready',
      pending: String(
        sourceConversations.reduce(
          (total, chat) =>
            total + chat.messages.filter((message) => message.delivery === 'sending').length,
          0,
        ),
      ),
      error: error,
    },
    conversations,
    integrity_warnings: Object.fromEntries(
      sourceConversations.flatMap((chat) =>
        chat.integrity_error ? [[chat.id, chat.integrity_error]] : [],
      ),
    ),
    has_unreadable_local_history: false,
    requests: [],
    agents: [],
  };
}
