import type { NativeNotificationsProps, NativeProfileProps, NativeRootPersona, NativeRootRequest } from './native-root-screens.tsx';
import type { ConnectionState } from './transport.ts';

/** Public view data only. Null means this client has no authoritative projection. */
export type NativeRootScreenData = Pick<NativeNotificationsProps, 'agents' | 'mentions' | 'items'> &
  { requests: (NativeRootRequest & { spaceId: string })[] | null } &
  Pick<NativeProfileProps, 'me' | 'stats' | 'grants' | 'account' | 'connection' | 'syncMode' | 'plannerLabel'> &
  { contacts: NativeRootPersona[] | null };

export function browserRootData(account: { identity: string; device: string; name: string; handle: string },
  connection: ConnectionState): NativeRootScreenData {
  const me: NativeRootPersona = { kind: 'person', id: account.identity, name: account.name, handle: account.handle };
  return {
    me, contacts: null, requests: null, agents: null, mentions: null, items: null, stats: null, grants: null,
    account: { name: account.name, handle: account.handle, deviceId: account.device }, syncMode: 'live',
    connection: {
      state: connection.kind === 'ready' ? 'online' : connection.kind === 'connecting' || connection.kind === 'authenticating' ? 'connecting' : 'offline',
      // Authentication is not proof of SyncDone or an empty durable outbox.
      synced: null, pending: null, error: connection.kind === 'failed' ? connection.message : null,
    },
  };
}
