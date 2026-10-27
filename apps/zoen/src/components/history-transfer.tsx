/* oxlint-disable jsx-a11y/prefer-tag-over-role -- React Native Text supports role="status"; HTML output is not a native component. */
import { t } from '../i18n';
import { useCallback, useRef, useState } from 'react';
import { useFocusEffect } from 'expo-router';
import { AppState, Text } from 'react-native';
import { useClient } from '../core/provider';
import { accountScope, accountScopeKey } from '../core/account-scope';
import type { HistoryStatusDto } from '../core/generated/HistoryStatusDto';
import { useTheme } from '../theme';
import { NativeButton, Surface } from './primitives';

/** The core owns transfer progress and resumption across screens and cold starts. */
export function HistoryTransfer() {
  const client = useClient();
  const { store } = client;
  const { colors } = useTheme();
  const owner = accountScopeKey(accountScope(store, client));
  const lease = useRef({ generation: 0, owner: '', active: false });
  const busyNow = useRef(false);
  const [revision, setRevision] = useState(0);
  const [state, setState] = useState<{
    owner: string;
    status?: HistoryStatusDto;
    error?: string;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const current = useCallback(
    (generation: number) =>
      lease.current.active &&
      generation === lease.current.generation &&
      lease.current.owner === accountScopeKey(accountScope(store)),
    [store],
  );

  useFocusEffect(
    useCallback(() => {
      let timer: ReturnType<typeof setTimeout> | undefined;
      let foreground = AppState.currentState === 'active';
      const retire = () => {
        lease.current = { generation: lease.current.generation + 1, owner: '', active: false };
        clearTimeout(timer);
        busyNow.current = false;
        setBusy(false);
        setState(null);
      };
      async function read(generation: number) {
        if (!current(generation)) return;
        try {
          const status = await store.driver.request({ op: 'history_status' });
          if (!current(generation)) return;
          setState({ owner, status });
          if (status.initial_pending || status.receive_active)
            timer = setTimeout(() => void read(generation), 2_000);
        } catch (reason) {
          if (current(generation))
            setState({
              owner,
              error: reason instanceof Error ? reason.message : t('access.checkHistoryFailed'),
            });
        }
      }
      const activate = () => {
        retire();
        if (!foreground || !owner || owner !== accountScopeKey(accountScope(store))) return;
        lease.current = { generation: lease.current.generation, owner, active: true };
        void read(lease.current.generation);
      };
      activate();
      const unsubscribe = store.subscribe(() => {
        if (accountScopeKey(accountScope(store)) !== owner) retire();
      });
      const preflight = store.registerSessionPreflight(retire);
      const app = AppState.addEventListener('change', (next) => {
        if ((next === 'active') === foreground) return;
        foreground = next === 'active';
        if (foreground) activate();
        else retire();
      });
      return () => {
        app.remove();
        unsubscribe();
        preflight();
        retire();
      };
      // A manual attempt restarts this focused read after its bounded receive.
      // oxlint-disable-next-line react/exhaustive-deps
    }, [store, owner, current, revision]),
  );

  const renderedGeneration = lease.current.generation;
  if (!owner || state?.owner !== owner) return null;
  const { status, error } = state;
  if (!error && !status?.initial_pending && !status?.receive_active) return null;

  async function retry() {
    const generation = renderedGeneration;
    if (busyNow.current || !current(generation)) return;
    busyNow.current = true;
    setBusy(true);
    try {
      const latest = await store.driver.request({ op: 'history_status' });
      if (!current(generation)) return;
      if (latest.initial_pending && !latest.receive_active)
        await store.driver.request({ op: 'receive_history', timeout_ms: 15_000 });
      if (current(generation)) await store.refresh();
    } catch (reason) {
      if (current(generation))
        setState({
          owner,
          status,
          error: reason instanceof Error ? reason.message : t('access.transferContinuesLater'),
        });
    } finally {
      if (current(generation)) {
        busyNow.current = false;
        setBusy(false);
        setRevision((value) => value + 1);
      }
    }
  }

  return (
    <Surface>
      <Text
        accessibilityRole="header"
        style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}
      >
        {t('access.historyOnDevice')}
      </Text>
      <Text
        role="status"
        accessibilityLiveRegion="polite"
        style={{ color: colors.secondary, lineHeight: 24 }}
      >
        {status?.manifest_received
          ? t('access.partsReceived', {
              received: status.chunks_received,
              count: status.chunks_total,
            })
          : t('access.historyAutoResume')}
      </Text>
      {error ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger, lineHeight: 24 }}>
          {error}
        </Text>
      ) : null}
      <NativeButton
        secondary
        label={
          busy || status?.receive_active ? t('access.historyReceiving') : t('access.tryReceiveNow')
        }
        disabled={busy || status?.receive_active === true}
        onPress={() => void retry()}
      />
    </Surface>
  );
}
