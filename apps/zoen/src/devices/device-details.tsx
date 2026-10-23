import { BottomSheet, RNHostView } from '@expo/ui';
import { Text, View } from 'react-native';
import type { DeviceDto } from '../core/generated/DeviceDto';
import { IconButton, NativeButton } from '../components/primitives';
import { useTheme } from '../theme';
import { t } from '../i18n';

export const deviceName = (device: DeviceDto) =>
  device.this_device
    ? t('deviceDirectory.current')
    : t('settings.deviceSuffix', { suffix: device.device_id.slice(-8) });

export function DeviceDetails({
  device,
  pending,
  onClose,
  onRevoke,
}: {
  device: DeviceDto | null;
  pending: boolean;
  onClose(): void;
  onRevoke(): void;
}) {
  const { presentation: p } = useTheme();
  return (
    <BottomSheet
      testID="device-details-sheet"
      isPresented={Boolean(device)}
      onDismiss={onClose}
      snapPoints={['half', 'full']}
      containerColor={p.list.background}
    >
      <RNHostView matchContents>
        <View>
          {device ? (
            <View style={{ gap: 24, paddingVertical: 16 }}>
              <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
                <Text
                  accessibilityRole="header"
                  style={{ flex: 1, color: p.list.text, fontSize: 24, fontWeight: '600' }}
                >
                  {deviceName(device)}
                </Text>
                <IconButton
                  name={{ ios: 'xmark', android: 'close', web: 'close' }}
                  label={t('deviceDirectory.close')}
                  onPress={onClose}
                />
              </View>
              {device.revoked ? (
                <Text style={{ color: p.destructive }}>{t('settings.accessRevoked')}</Text>
              ) : null}
              <View style={{ gap: 8 }}>
                <Text style={{ color: p.list.secondary, fontSize: 13 }}>
                  {t('deviceDirectory.fingerprint')}
                </Text>
                <Text
                  selectable
                  style={{
                    color: p.list.text,
                    fontSize: 16,
                    lineHeight: 25,
                    fontVariant: ['tabular-nums'],
                  }}
                >
                  {device.device_id.match(/.{1,4}/g)?.join(' ')}
                </Text>
              </View>
              {!device.this_device && !device.revoked ? (
                <NativeButton
                  testID="device-revoke"
                  destructive
                  label={pending ? t('settings.pleaseWait') : t('deviceDirectory.revoke')}
                  disabled={pending}
                  onPress={onRevoke}
                />
              ) : null}
            </View>
          ) : null}
        </View>
      </RNHostView>
    </BottomSheet>
  );
}
