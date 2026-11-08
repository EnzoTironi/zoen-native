import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { randomUUID } from 'expo-crypto';
import { useClient } from '../core/provider';
import { conversationScope } from '../core/conversation-scope';
import { MessagePinsSession } from './session';
export function useMessagePins(
  space: string,
  message: string | null = null,
  expectedOwner?: string,
) {
  const client = useClient(),
    { store, snapshot } = client;
  const owner = conversationScope(client, store.sessionEpoch, space);
  const account = snapshot?.account;
  const identity = account?.identity_id ?? '',
    device = account?.device_id ?? '',
    relay = account?.relay_url ?? '';
  const session = useMemo(
    () =>
      new MessagePinsSession(
        {
          expected_identity_id: identity,
          expected_device_id: device,
          expected_relay_url: relay,
          space_id: space,
        },
        message,
        {
          current: () =>
            !store.isSessionRetiring &&
            store.getSnapshot().mode === 'account' &&
            !!owner &&
            (!expectedOwner || owner === expectedOwner) &&
            conversationScope(store.getSnapshot(), store.sessionEpoch, space) === owner,
          page: (scope, cursor) =>
            store.driver.request({ op: 'message_pins', scope, cursor, limit: 50 }),
          review: (scope, message_id) =>
            store.driver.request({ op: 'message_pin_review', scope, message_id }),
          write: async (input) => {
            const result = await store.driver.request({ op: 'set_message_pin', input });
            if (conversationScope(store.getSnapshot(), store.sessionEpoch, space) === owner)
              void store.refresh([space]).catch(() => {});
            return result;
          },
          operationId: randomUUID,
        },
      ),
    [store, space, message, owner, expectedOwner, identity, device, relay],
  );
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      if (foreground) session.focus();
      const app = AppState.addEventListener('change', (next) => {
        if ((next === 'active') === foreground) return;
        foreground = next === 'active';
        if (foreground) session.focus();
        else session.blur();
      });
      const scope = store.subscribe(() => session.check());
      const preflight = store.registerSessionPreflight(() => session.blur());
      const canonical = store.subscribeCanonicalChanges((spaces) => {
        if (foreground && (!spaces || spaces.includes(space))) void session.refresh();
      });
      return () => {
        app.remove();
        scope();
        preflight();
        canonical();
        session.blur();
      };
    }, [session, store, space]),
  );
  return { session, state, owner };
}
