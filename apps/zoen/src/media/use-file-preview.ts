import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useFocusEffect } from 'expo-router';
import { AppState } from 'react-native';
import { useClient } from '../core/provider';
import { documentScope } from '../documents/document-authority';
import { homeSpacesFor } from '../home/access';
import type { MediaDriver } from './types';

export function useFilePreview<S extends { close(): Promise<void> }>({
  media,
  spaceId,
  isCurrent,
  create,
}: {
  media: MediaDriver;
  spaceId?: string;
  isCurrent(): boolean;
  create(current: () => boolean): S;
}) {
  const { store } = useClient();
  const externalCurrent = useRef(isCurrent);
  useLayoutEffect(() => {
    externalCurrent.current = isCurrent;
  }, [isCurrent]);
  const [scope] = useState(() => documentScope(store.getSnapshot(), store.sessionEpoch));
  const focus = useRef(false);
  const live = useRef<object | null>(null);
  const [activation, setActivation] = useState<object | null>(null);
  const current = () =>
    Boolean(
      scope &&
      live.current &&
      !store.isSessionRetiring &&
      store.driver.media === media &&
      documentScope(store.getSnapshot(), store.sessionEpoch) === scope &&
      (!spaceId || homeSpacesFor(store.getSnapshot()).has(spaceId)) &&
      externalCurrent.current(),
    );
  const [session] = useState(() => create(current));
  useFocusEffect(
    useCallback(() => {
      focus.current = true;
      const token = AppState.currentState === 'active' ? {} : null;
      live.current = token;
      setActivation(token);
      return () => {
        focus.current = false;
        live.current = null;
        void session.close().catch(() => {});
      };
    }, [session]),
  );
  useEffect(() => {
    const listener = AppState.addEventListener('change', (next) => {
      const token = next === 'active' && focus.current ? {} : null;
      live.current = token;
      setActivation(token);
      void session.close().catch(() => {});
    });
    const detach = store.subscribe(() => {
      if (!current()) void session.close().catch(() => {});
    });
    const unregister = store.registerSessionCleanup(() => session.close());
    return () => {
      live.current = null;
      listener.remove();
      detach();
      unregister();
      void session.close().catch(() => {});
    };
    // Session captures the original file/owner/driver; prop changes remount its caller.
    // oxlint-disable-next-line react/exhaustive-deps
  }, [session, store]);
  return {
    session,
    current: () => Boolean(activation && activation === live.current && current()),
  };
}
