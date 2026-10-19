import { t } from '../i18n/index.ts';
import { useState } from 'react';
import { Stack } from 'expo-router';
import { ScrollView, Text, View } from 'react-native';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { useTheme } from '../theme';
import { useDialogs } from '../components/dialogs';
import { EmptyState, Icon, Loading, NativeButton, Surface } from '../components/primitives';
import { AuthorizeDevice } from '../components/authorize-device';
import { HistoryTransfer } from '../components/history-transfer';

export default function Devices() {
  const { store, mode, snapshot } = useClient();
  const { data, error, retry } = useCoreQuery(mode === 'account' ? { op: 'devices' } : null);
  const { colors } = useTheme();
  const { confirm } = useDialogs();
  const [busy, setBusy] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  async function unlink(device: string) {
    if (busy) return;
    setBusy(device);
    setFailure(null);
    try {
      const accepted = await confirm({
        title: t('settings.unlinkThisDevice'),
        message: t('settings.itWillLoseAccessToTheAccountAnd'),
        confirmLabel: t('settings.unlink'),
        destructive: true,
      });
      if (!accepted) return;
      await store.driver.request({ op: 'unlink_device', device_id: device });
      retry();
    } catch (reason) {
      setFailure(
        reason instanceof Error ? reason.message : t('settings.theDeviceCouldNotBeUnlinked'),
      );
    } finally {
      setBusy(null);
    }
  }
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{ padding: 24, gap: 20 }}
    >
      <Stack.Screen options={{ title: t('settings.devices') }} />
      {mode === 'account' ? <AuthorizeDevice onLinked={retry} /> : null}
      {mode === 'account' ? (
        <HistoryTransfer
          key={`${snapshot?.account?.identity_id}:${snapshot?.account?.device_id}`}
        />
      ) : null}
      <Text style={{ color: colors.secondary, lineHeight: 24, fontSize: 16 }}>
        {t('settings.reviewTheDevicesAuthorizedToAccessYourAccount')}
      </Text>
      {failure || error ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
          {failure ?? error}
        </Text>
      ) : null}
      {mode === 'demo' ? (
        <EmptyState
          title={t('settings.localDemo')}
          detail={t('settings.linkedDevicesAppearWhenYouUseAnAccount')}
        />
      ) : !data && !error ? (
        <Loading />
      ) : (
        data?.map((device) => (
          <Surface key={device.device_id}>
            <View style={{ flexDirection: 'row', gap: 14, alignItems: 'center' }}>
              <Icon
                name={{
                  ios: device.this_device ? 'iphone' : 'laptopcomputer',
                  android: 'devices',
                  web: 'devices',
                }}
                size={28}
                color={device.revoked ? colors.tertiary : colors.accent}
              />
              <View style={{ flex: 1, gap: 5 }}>
                <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
                  {device.this_device
                    ? t('settings.thisDevice')
                    : t('settings.deviceSuffix', { suffix: device.device_id.slice(-8) })}
                </Text>
                <Text style={{ color: device.revoked ? colors.danger : colors.secondary }}>
                  {device.revoked ? t('settings.accessRevoked') : t('settings.authorized')}
                </Text>
              </View>
            </View>
            <Text selectable style={{ color: colors.tertiary, fontSize: 12, lineHeight: 18 }}>
              {device.device_id}
            </Text>
            {!device.this_device && !device.revoked ? (
              <NativeButton
                secondary
                label={busy === device.device_id ? t('settings.pleaseWait') : t('settings.unlink')}
                disabled={busy !== null}
                onPress={() => {
                  void unlink(device.device_id);
                }}
              />
            ) : null}
          </Surface>
        ))
      )}
      {mode === 'account' ? (
        <NativeButton
          secondary
          label={t('settings.refreshDevices')}
          disabled={busy !== null}
          onPress={retry}
        />
      ) : null}
    </ScrollView>
  );
}
