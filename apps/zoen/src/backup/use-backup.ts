import { useCallback, useMemo, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import * as Clipboard from 'expo-clipboard';
import { useClient } from '../core/provider';
import { accountScope, accountScopeKey, type AccountScope } from '../core/account-scope';
import { BackupSession, emptyBackup } from './session';
export function useBackup() {
  const client = useClient(),
    { store } = client;
  const scope = accountScopeKey(accountScope(store, client));
  const session = useMemo(() => {
    const owner = () => accountScope(store);
    const native = (value: AccountScope) => ({
      expected_identity_id: value.identity,
      expected_device_id: value.device,
      expected_relay_url: value.relay,
    });
    return new BackupSession({
      owner,
      status: (value) => store.driver.request({ op: 'backup_status', scope: native(value) }),
      generate: (value) =>
        store.driver.request({ op: 'backup_create_recovery_key', scope: native(value) }),
      enable: (value, key) =>
        store.driver.request({
          op: 'backup_enable_recovery_key',
          scope: native(value),
          recovery_key: key,
        }),
      password: (value, password) =>
        store.driver.request({ op: 'backup_password', scope: native(value), password }),
      upload: (value) => store.driver.request({ op: 'backup_now', scope: native(value) }),
      disable: (value) => store.driver.request({ op: 'backup_turn_off', scope: native(value) }),
      copy: (key) => Clipboard.setStringAsync(key, { android: { isSensitive: true } }),
      schedule: (callback) => {
        const timer = setTimeout(callback, 1000);
        return () => clearTimeout(timer);
      },
    });
  }, [store]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  useFocusEffect(
    useCallback(() => {
      let foreground = AppState.currentState === 'active';
      const activate = () => {
        const current = accountScope(store);
        session.activate(foreground && accountScopeKey(current) === scope ? current : null);
      };
      activate();
      const unsubscribe = store.subscribe(() => {
        if (accountScopeKey(accountScope(store)) !== scope) session.retire();
      });
      const preflight = store.registerSessionPreflight(() => session.retire());
      const app = AppState.addEventListener('change', (value) => {
        if ((value === 'active') === foreground) return;
        foreground = value === 'active';
        if (foreground) activate();
        else session.retire();
      });
      return () => {
        unsubscribe();
        preflight();
        app.remove();
        session.retire();
      };
    }, [store, scope, session]),
  );
  return { session, state: state.scope === scope ? state : emptyBackup, accountRequired: !scope };
}
