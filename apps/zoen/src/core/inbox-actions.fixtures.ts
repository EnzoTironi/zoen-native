import { InboxActionsSession, type InboxActionsPort } from '../inbox/actions-session.ts';
import { mutedSnapshot, scope } from './muted-chats.fixtures.ts';
import type { ChatLayoutInput } from './generated/ChatLayoutInput';
import type { ChatLayoutResult } from './generated/ChatLayoutResult';
import type { AccountScope } from './account-scope.ts';
export { deferred } from './store.fixtures.ts';
export { scope };
export function inboxFixture() {
  const snapshot = mutedSnapshot();
  const chat = snapshot.conversations[0]!;
  chat.muted_until_ms = '1900000000000';
  chat.pinned = false;
  chat.archived = false;
  chat.unread = 3;
  let owner: AccountScope | null = scope,
    sequence = 0;
  let current = { pinned: false, archived: false, pin_revision: '0', archive_revision: '0' };
  const receipts = new Map<string, NonNullable<ChatLayoutResult['receipt']>>();
  const calls: Array<
    | ChatLayoutInput
    | { kind: 'mute'; id: string; value: boolean; observed: string | null }
    | { kind: 'read' | 'refresh' | 'snapshot' | 'newId' }
  > = [];
  let read = async () => structuredClone(snapshot);
  let layout = async (input: ChatLayoutInput): Promise<ChatLayoutResult> => {
    if (input.kind === 'read') return { current: { ...current }, receipt: null };
    if (input.kind === 'reconcile')
      return { current: { ...current }, receipt: receipts.get(input.operation_id) ?? null };
    const revision =
      input.kind === 'change'
        ? input.field === 'pinned'
          ? current.pin_revision
          : current.archive_revision
        : receipts.get(input.original_operation_id)?.revision;
    const original = input.kind === 'undo' ? receipts.get(input.original_operation_id) : null;
    const field = input.kind === 'change' ? input.field : original!.field;
    if (
      input.observed_revision !== revision ||
      (input.kind === 'undo' &&
        (field === 'pinned' ? current.pin_revision : current.archive_revision) !==
          input.observed_revision)
    )
      throw Error('Changed revision');
    const value = input.kind === 'change' ? input.value : original!.before;
    const next = `test-revision-${input.operation_id}`;
    const receipt = {
      operation_id: input.operation_id,
      field,
      before: current[field],
      value,
      revision: next,
      undo_of: original?.operation_id ?? null,
      undo_available: input.kind === 'change',
    };
    current = {
      ...current,
      [field]: value,
      [field === 'pinned' ? 'pin_revision' : 'archive_revision']: next,
    };
    chat[field] = value;
    receipts.set(input.operation_id, receipt);
    return { current: { ...current }, receipt };
  };
  const canonical = layout;
  const port: InboxActionsPort = {
    scope: () => owner,
    member: (id) =>
      Boolean(
        owner &&
        snapshot.conversations.some(
          (item) => item.id === id && item.members.some((m) => m.id === owner!.identity),
        ),
      ),
    snapshot: () => {
      calls.push({ kind: 'snapshot' });
      return read();
    },
    layout: (input) => {
      calls.push(structuredClone(input));
      return layout(input);
    },
    newId: async () => {
      calls.push({ kind: 'newId' });
      return `intent-${++sequence}`;
    },
    mute: async (id, value, observed) => {
      calls.push({ kind: 'mute', id, value, observed });
      chat.muted = value;
      chat.muted_until_ms = value ? '9223372036854775807' : null;
    },
    read: async () => {
      calls.push({ kind: 'read' });
      chat.unread = 0;
    },
    refresh: async () => {
      calls.push({ kind: 'refresh' });
    },
  };
  const session = new InboxActionsSession(port);
  session.activate(owner);
  return {
    session,
    snapshot,
    chat,
    port,
    calls,
    receipts,
    canonical,
    owner: (value: AccountScope | null) => {
      owner = value;
    },
    read: (fn: typeof read) => {
      read = fn;
    },
    layout: (fn: typeof layout) => {
      layout = fn;
    },
    current: () => current,
    setCurrent: (patch: Partial<typeof current>) => {
      current = { ...current, ...patch };
    },
    run: (action: 'archive' | 'pin' | 'mute' | 'read') =>
      session.run(structuredClone(chat), action, session.ticket()),
    undo: () => session.undo(session.getSnapshot().undo!, session.ticket()),
  };
}
