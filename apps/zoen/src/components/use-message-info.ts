import { AppState } from 'react-native';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useFocusEffect } from 'expo-router';
import type { MessageInfo } from '../core/generated/MessageInfo';
import { MessageInfoSession } from '../core/message-info-session';
import { conversationScope } from '../core/conversation-scope';
import { useClient } from '../core/provider';
import { homeOwnerFor } from '../home/access';

const loadingState = { value: null, loading: true, error: false };

export function useMessageInfo(spaceId: string, messageId: string, expectedScope: string) {
  const { store } = useClient();
  const [state, setState] = useState<{
    session: MessageInfoSession;
    value: MessageInfo | null;
    loading: boolean;
    error: boolean;
  } | null>(null);
  const session = useMemo(() => {
    const owner = homeOwnerFor(store.getSnapshot());
    const epoch = store.sessionEpoch;
    const session = new MessageInfoSession(
      {
        expected_identity_id: owner?.identityId ?? '',
        expected_device_id: owner?.deviceId ?? '',
        space_id: spaceId,
        message_id: messageId,
      },
      {
        current: () =>
          !store.isSessionRetiring &&
          Boolean(expectedScope) &&
          conversationScope(store.getSnapshot(), store.sessionEpoch, spaceId) === expectedScope &&
          store.sessionEpoch === epoch,
        read: (scope) => store.driver.request({ op: 'message_info', scope }),
      },
      (value, loading, error) => setState({ session, value, loading, error }),
    );
    return session;
  }, [store, spaceId, messageId, expectedScope]);
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      let retired = false;
      const activate = () => {
        if (foreground) session.focus();
        else session.blur();
      };
      activate();
      const app = AppState.addEventListener('change', (next) => {
        if (retired) return;
        const active = next === 'active';
        if (foreground === active) return;
        foreground = active;
        activate();
      });
      return () => {
        retired = true;
        app.remove();
        session.blur();
      };
    }, [session]),
  );
  useEffect(() => {
    const authority = store.subscribe(() => session.checkAuthority());
    const canonical = store.subscribeCanonicalChanges((spaces) => {
      if (!spaces || spaces.includes(spaceId)) session.refresh();
    });
    return () => {
      authority();
      canonical();
    };
  }, [store, session, spaceId]);
  const visible = state?.session === session ? state : loadingState;
  return {
    value: visible.value,
    loading: visible.loading,
    error: visible.error,
    retry: () => session.refresh(),
  };
}
