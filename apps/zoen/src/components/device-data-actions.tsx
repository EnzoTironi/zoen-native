import { t } from '../i18n/index.ts';
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
        title: t('settings.signOutOfThisDevice'),
        message:
          pending && pending !== '0'
            ? t('settings.signOutWithPending')
            : t('settings.chatsAndKeysOnThisDeviceWillBe'),
        confirmLabel: t('settings.signOutAndEraseData'),
        destructive: true,
      });
      if (accepted && active.current) await store.eraseDevice(expected);
    } catch (error) {
      if (active.current)
        await inform(
          t('settings.couldNotSignOut'),
          error instanceof Error ? error.message : t('settings.tryAgain'),
        );
    } finally {
      busy.current = false;
      if (active.current) setConfirming(false);
    }
  }

  return (
    <Surface>
      <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
        {t('settings.dataOnThisDevice')}
      </Text>
      <Text style={{ color: colors.secondary, lineHeight: 22 }}>
        {t('settings.signingOutErasesChatsAndKeysOnThis')}
      </Text>
      <NativeButton
        label={t('settings.signOutAndEraseData')}
        testID="erase-device"
        destructive
        disabled={confirming}
        onPress={() => void signOut()}
      />
    </Surface>
  );
}
