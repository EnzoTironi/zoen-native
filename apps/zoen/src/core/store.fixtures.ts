import type {
  Command,
  CommandResult,
  CoreDriver,
  CoreEvent,
  Snapshot,
  TimelineEntry,
} from './types.ts';

export const empty: Snapshot = {
  account: null,
  me: null,
  connection: { state: 'offline', synced: false, pending: '0', error: null },
  conversations: [],
  integrity_warnings: {},
  has_unreadable_local_history: false,
  requests: [],
  agents: [],
};
export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((done, failed) => {
    resolve = done;
    reject = failed;
  });
  return { promise, resolve, reject };
}
export const tick = () => new Promise<void>((resolve) => setImmediate(resolve));

export class Driver implements CoreDriver {
  durableMessageDrafts?: boolean;
  opening = async () => empty;
  operation: (command: Command) => Promise<unknown> = async () => empty;
  changed: ((event: CoreEvent) => void) | undefined;
  async open() {
    return this.opening();
  }
  async request<C extends Command>(command: C): Promise<CommandResult<C>> {
    return (await this.operation(command)) as CommandResult<C>;
  }
  async createAccount() {
    return empty;
  }
  subscribe(listener: (event: CoreEvent) => void) {
    this.changed = listener;
    return () => {
      if (this.changed === listener) this.changed = undefined;
    };
  }
  async close(_options?: { force?: boolean }) {}
  async eraseDevice() {}
}

export const entry = (id: string) => ({ id }) as TimelineEntry;

export const erasureAccount = {
  identity_id: 'a'.repeat(64),
  device_id: 'b'.repeat(64),
  name: 'Synthetic',
  handle: 'qa_erase',
  relay_url: 'https://relay.example',
  registered: true,
  unlocked: true,
  setup_pending: false,
};
export const erasureScope = {
  identity_id: erasureAccount.identity_id,
  device_id: erasureAccount.device_id,
};

export function unreadSnapshot(unread: number): Snapshot {
  return {
    ...empty,
    account: erasureAccount,
    conversations: [
      {
        id: 'read-chat',
        title: 'Read QA',
        kind: 'Group',
        privacy: 'EndToEnd',
        members: [],
        counterpart: null,
        last_preview: 'A received file',
        last_author: null,
        last_at_ms: '1',
        unread,
        last_delivery: null,
        pinned: false,
        archived: false,
        muted: false,
        muted_until_ms: null,
        pending_requests: 0,
        event_count: '2',
      },
    ],
  };
}

export function draftSession(): Snapshot {
  const snapshot = unreadSnapshot(0);
  return {
    ...snapshot,
    conversations: snapshot.conversations.map((conversation) => ({
      ...conversation,
      members: [
        {
          id: erasureAccount.identity_id,
          kind: 'Person',
          name: 'QA',
          handle: 'qa',
          initials: 'QA',
          tint_hex: '#194D2A',
          glyph: null,
          bio: '',
          owner_id: null,
          owner_name: null,
          owner_tint_hex: null,
          photo_sha256: null,
          photo_ready: false,
          photo_mime: null,
          is_me: true,
          is_mine: false,
        },
      ],
    })),
  };
}
