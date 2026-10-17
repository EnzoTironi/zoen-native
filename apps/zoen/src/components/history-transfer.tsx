import { useCallback, useRef, useState } from 'react';
import { useFocusEffect } from 'expo-router';
import { Text } from 'react-native';
import { useClient } from '../core/provider';
import type { HistoryStatusDto } from '../core/generated/HistoryStatusDto';
import { useTheme } from '../theme';
import { NativeButton, Surface } from './primitives';

/** The core owns transfer progress and resumption across screens and cold starts. */
export function HistoryTransfer() {
  const { store, mode, snapshot } = useClient();
  const { colors } = useTheme();
  const identity = snapshot?.account?.identity_id;
  const epoch = useRef(0);
  const [revision, setRevision] = useState(0);
  const [state, setState] = useState<{
    identity: string;
    status?: HistoryStatusDto;
    error?: string;
  } | null>(null);
  const [busy, setBusy] = useState(false);

  useFocusEffect(
    useCallback(() => {
      const generation = ++epoch.current;
      let timer: ReturnType<typeof setTimeout> | undefined;
      if (!identity || mode !== 'account') return;
      setBusy(false);
      const accountIdentity = identity;
      async function read() {
        try {
          const status = await store.driver.request({ op: 'history_status' });
          if (epoch.current !== generation) return;
          setState({ identity: accountIdentity, status });
          if (status.initial_pending || status.receive_active)
            timer = setTimeout(() => void read(), 2_000);
        } catch (reason) {
          if (epoch.current === generation)
            setState({
              identity: accountIdentity,
              error:
                reason instanceof Error
                  ? reason.message
                  : 'Não foi possível verificar o histórico.',
            });
        }
      }
      void read();
      return () => {
        ++epoch.current;
        clearTimeout(timer);
      };
      // A manual attempt restarts this focused read after its bounded receive.
      // oxlint-disable-next-line react/exhaustive-deps
    }, [store, identity, mode, revision]),
  );

  if (!identity || state?.identity !== identity) return null;
  const { status, error } = state;
  if (!error && !status?.initial_pending && !status?.receive_active) return null;

  async function retry() {
    if (busy) return;
    const generation = epoch.current;
    setBusy(true);
    try {
      const current = await store.driver.request({ op: 'history_status' });
      if (generation !== epoch.current) return;
      if (current.initial_pending && !current.receive_active)
        await store.driver.request({ op: 'receive_history', timeout_ms: 15_000 });
      if (generation === epoch.current) await store.refresh();
    } catch (reason) {
      if (generation === epoch.current)
        setState({
          identity: identity!,
          status,
          error: reason instanceof Error ? reason.message : 'A transferência continuará depois.',
        });
    } finally {
      if (generation === epoch.current) {
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
        Seu histórico neste dispositivo
      </Text>
      <Text accessibilityRole="status" style={{ color: colors.secondary, lineHeight: 24 }}>
        {status?.manifest_received
          ? `${status.chunks_received} de ${status.chunks_total} partes recebidas. As conversas serão abertas após a conferência.`
          : 'Mantenha o outro dispositivo conectado para enviar o histórico. A transferência é retomada automaticamente.'}
      </Text>
      {error ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger, lineHeight: 24 }}>
          {error}
        </Text>
      ) : null}
      <NativeButton
        secondary
        label={busy || status?.receive_active ? 'Recebendo histórico…' : 'Tentar receber agora'}
        disabled={busy || status?.receive_active === true}
        onPress={() => void retry()}
      />
    </Surface>
  );
}
