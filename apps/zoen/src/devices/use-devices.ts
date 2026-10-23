import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { useClient } from '../core/provider';
import { accountScopeKey } from '../core/account-scope';
import { createDevicePort, DeviceSession, emptyDevices } from './session';

export function useDevices() {
  const { store } = useClient();
  const port = useMemo(() => createDevicePort(store), [store]);
  const session = useMemo(() => new DeviceSession(port), [port]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const key = accountScopeKey(port.scope());
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      const activate = () => {
        const owner = port.scope();
        session.activate(foreground && accountScopeKey(owner) === key ? owner : null);
        if (foreground) void session.load(session.ticket());
      };
      activate();
      const unsubscribe = store.subscribe(() => {
        if (accountScopeKey(port.scope()) !== key) session.retire();
      });
      const preflight = store.registerSessionPreflight(() => session.retire());
      const app = AppState.addEventListener('change', (next) => {
        if ((next === 'active') === foreground) return;
        foreground = next === 'active';
        if (foreground) activate();
        else session.retire();
      });
      return () => {
        app.remove();
        unsubscribe();
        preflight();
        session.retire();
      };
    }, [store, port, session, key]),
  );
  return {
    session,
    accountRequired: !key,
    state: state.scope === key ? state : { ...emptyDevices, scope: key },
  };
}
