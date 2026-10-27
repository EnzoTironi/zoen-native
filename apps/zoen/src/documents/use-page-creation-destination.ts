import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { accountScope, accountScopeKey } from '../core/account-scope';
import type { ClientState, ClientStore } from '../core/store';
import { homeSpacesFor } from '../home/access';
import { documentScope } from './document-authority';
import { DocumentExportActivity } from './export-activity';

export function pageCreationScope(
  store: ClientStore,
  state: ClientState = store.getSnapshot(),
): string {
  return state.mode === 'demo'
    ? (documentScope(state, store.sessionEpoch) ?? '')
    : accountScopeKey(accountScope(store, state));
}

/** Read-only destination projection and route leases; Core still admits each creation. */
export function usePageCreationDestination(initial: string) {
  const client = useClient(),
    { store } = client;
  const scope = pageCreationScope(store, client);
  const owner = accountScope(store, client);
  const [space, setSpace] = useState(initial);
  const selected = useRef(initial);
  const [activity] = useState(() => new DocumentExportActivity());
  const [view, setView] = useState<{ focused: boolean; current(): boolean }>({
    focused: false,
    current: () => false,
  });
  const [busy, setBusy] = useState(false);
  const [revision, setRevision] = useState(0);
  const [pendingSnapshot, setPendingSnapshot] = useState<typeof client.snapshot>(null);
  const stampNow = useRef<string | null>(null);
  const retireNow = useRef<() => void>(() => {});
  const chat = client.snapshot?.conversations.find((c) => c.id === space);
  const requiresRole = chat?.kind === 'Group' || chat?.kind === 'Community';
  const read = useCoreQuery(
    requiresRole && owner
      ? {
          op: 'space_member_context',
          space_id: space,
          identity_id: owner.identity,
          expected_identity_id: owner.identity,
          expected_device_id: owner.device,
        }
      : null,
    `${scope}:${revision}`,
  );
  useFocusEffect(
    useCallback(() => {
      let release = () => {};
      let foreground = AppState.currentState === 'active';
      const retire = () => {
        release();
        setBusy(false);
        setView({ focused: false, current: () => false });
      };
      const activate = () => {
        release();
        if (scope && pageCreationScope(store) === scope && AppState.currentState === 'active') {
          release = activity.focus();
          setView({ focused: true, current: activity.observe() });
        }
        setBusy(false);
      };
      retireNow.current = retire;
      activate();
      const account = store.subscribe(() => {
        if (pageCreationScope(store) !== scope) retire();
      });
      const canonical = store.subscribeCanonicalChanges((spaces) => {
        if (!spaces || spaces.includes(space)) {
          setPendingSnapshot(store.getSnapshot().snapshot);
          stampNow.current = null;
          setRevision((n) => n + 1);
        }
      });
      const preflight = store.registerSessionPreflight(retire);
      const lifecycle = AppState.addEventListener('change', (next) => {
        const active = next === 'active';
        if (active === foreground) return;
        foreground = active;
        if (active) {
          stampNow.current = null;
          setRevision((n) => n + 1);
          activate();
        } else retire();
      });
      return () => {
        account();
        canonical();
        preflight();
        lifecycle.remove();
        release();
      };
    }, [activity, scope, space, store]),
  );
  const viewCurrent = () =>
    view.current() &&
    !!scope &&
    pageCreationScope(store) === scope &&
    selected.current === space &&
    AppState.currentState === 'active';
  const member = !space || homeSpacesFor(client).has(space);
  const context = read.data;
  const checking = requiresRole && read.loading;
  const roleAllowed =
    !!owner &&
    !read.loading &&
    !read.error &&
    !!context &&
    context.identity_id === owner.identity &&
    context.device_id === owner.device &&
    context.space_id === space &&
    context.member_id === owner.identity &&
    context.member?.id === owner.identity &&
    context.state === 'Active' &&
    /^[a-f0-9]{64}$/.test(context.revision) &&
    ['owner', 'admin', 'member'].includes(context.role ?? '');
  const fresh = pendingSnapshot !== client.snapshot;
  // Direct membership is its own contract; never call group member-context as a fallback.
  const allowed = !!chat && member && fresh && (chat.kind === 'Direct' || roleAllowed);
  const stamp = JSON.stringify([scope, space, revision, checking, read.error, context, member]);
  useLayoutEffect(() => {
    stampNow.current = stamp;
  }, [stamp]);
  const baseCurrent = () =>
    viewCurrent() && (!space || homeSpacesFor(store.getSnapshot()).has(space));
  return {
    space,
    busy,
    checking,
    allowed: allowed && view.focused,
    unavailable: !!space && !checking && (!allowed || !!read.error),
    failed: !!read.error,
    retry: () => {
      if (viewCurrent()) {
        stampNow.current = null;
        setRevision((n) => n + 1);
      }
    },
    choose(next: string) {
      if (
        !view.current() ||
        pageCreationScope(store) !== scope ||
        selected.current === next ||
        !homeSpacesFor(store.getSnapshot()).has(next)
      )
        return false;
      retireNow.current();
      selected.current = next;
      setSpace(next);
      return true;
    },
    begin(mode: 'picker' | 'create') {
      if (!baseCurrent() || (mode === 'create' && (!allowed || stampNow.current !== stamp)))
        return null;
      const flight = activity.begin();
      if (!flight) return null;
      setBusy(true);
      return {
        // A Core-accepted creation may refresh this projection; that is not a lost ACK.
        current: () =>
          flight.current() && baseCurrent() && (mode === 'create' || stampNow.current === stamp),
        finish: () => {
          if (flight.finish() && viewCurrent()) setBusy(false);
        },
      };
    },
  };
}
