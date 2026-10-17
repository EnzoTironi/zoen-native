import { useEffect, useRef, useState } from 'react';
import { Text } from 'react-native';
import { useClient } from '../core/provider';
import { useTheme } from '../theme';
import { useDialogs } from './dialogs';
import { NativeButton, Surface } from './primitives';

export function DeviceDataActions() {
  const { snapshot, store, mode } = useClient();
  const { colors } = useTheme();
  const { confirm, inform } = useDialogs();
  const active = useRef(true);
  const busy = useRef(false);
  const [confirming, setConfirming] = useState(false);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  const account = snapshot?.account;
  if (mode === 'demo' || !account || process.env.EXPO_OS === 'android') return null;

  async function signOut() {
    if (busy.current || !account) return;
    busy.current = true;
    setConfirming(true);
    const expected = { identity_id: account.identity_id, device_id: account.device_id };
    const pending = snapshot?.connection.pending;
    try {
      const accepted = await confirm({
        title: 'Sair deste aparelho?',
        message:
          'As conversas e as chaves deste aparelho serão apagadas. Sua conta e os outros aparelhos continuam ativos. Sem um backup ou outro aparelho conectado, este histórico não poderá ser recuperado.' +
          (pending && pending !== '0'
            ? '\n\nMensagens que ainda não foram enviadas também serão apagadas.'
            : ''),
        confirmLabel: 'Sair e apagar dados',
        destructive: true,
      });
      if (accepted && active.current) await store.eraseDevice(expected);
    } catch (error) {
      if (active.current)
        await inform(
          'Não foi possível sair',
          error instanceof Error ? error.message : 'Tente novamente.',
        );
    } finally {
      busy.current = false;
      if (active.current) setConfirming(false);
    }
  }

  return (
    <Surface>
      <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
        Dados neste aparelho
      </Text>
      <Text style={{ color: colors.secondary, lineHeight: 22 }}>
        Ao sair, as conversas e as chaves deste aparelho serão apagadas.
      </Text>
      <NativeButton
        label="Sair e apagar dados"
        testID="erase-device"
        destructive
        disabled={confirming}
        onPress={() => void signOut()}
      />
    </Surface>
  );
}
