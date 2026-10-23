import { Text, View } from 'react-native';
import type { DeviceDto } from '../core/generated/DeviceDto';
import { SettingsRows } from '../components/settings-rows';
import { EmptyState, Loading, NativeButton } from '../components/primitives';
import { ProfileSection } from '../profile/profile-section';
import { useTheme } from '../theme';
import { t } from '../i18n';
import { deviceName } from './device-details';
import type { DeviceSession, DeviceState, DeviceTicket } from './session';

export function DeviceList({
  state,
  session,
  ticket,
  accountRequired,
}: {
  state: DeviceState;
  session: DeviceSession;
  ticket: DeviceTicket | null;
  accountRequired: boolean;
}) {
  const { presentation: p } = useTheme();
  if (accountRequired)
    return (
      <EmptyState
        title={t('settings.yourAccount')}
        detail={t('settings.linkedDevicesAppearWhenYouUseAnAccount')}
      />
    );
  const rows = (devices: readonly DeviceDto[]) =>
    devices.map((device) => ({
      id: `device-${device.device_id}`,
      title: deviceName(device),
      icon: device.this_device
        ? { ios: 'iphone' as const, android: 'smartphone' as const, web: 'devices' as const }
        : { ios: 'key' as const, android: 'key' as const, web: 'key' as const },
      disabled: state.pending !== null || state.phase !== 'ready',
      onPress: () => session.select(device.device_id, ticket),
    }));
  const current = state.devices.filter((device) => device.this_device);
  const others = state.devices.filter((device) => !device.this_device && !device.revoked);
  const revoked = state.devices.filter((device) => !device.this_device && device.revoked);
  return (
    <View testID="device-directory-list" style={{ gap: 24 }}>
      {state.readFailed ? (
        <Text accessibilityRole="alert" style={{ color: p.destructive }}>
          {t('deviceDirectory.readFailed')}
        </Text>
      ) : null}
      {state.revokeFailed ? (
        <Text accessibilityRole="alert" style={{ color: p.destructive }}>
          {t('deviceDirectory.revokeFailed')}
        </Text>
      ) : null}
      {state.phase === 'loading' || state.phase === 'inactive' ? <Loading /> : null}
      {current.length ? <SettingsRows testID="device-current" rows={rows(current)} /> : null}
      {others.length ? (
        <ProfileSection title={t('deviceDirectory.others')}>
          <SettingsRows testID="device-others" rows={rows(others)} />
        </ProfileSection>
      ) : state.phase === 'ready' ? (
        <Text style={{ color: p.list.secondary, fontSize: 15 }}>
          {t('deviceDirectory.noOtherDevices')}
        </Text>
      ) : null}
      {revoked.length ? (
        <ProfileSection title={t('deviceDirectory.revoked')}>
          <SettingsRows testID="device-revoked" rows={rows(revoked)} />
        </ProfileSection>
      ) : null}
      <NativeButton
        testID="devices-refresh"
        secondary
        label={t(state.readFailed ? 'deviceDirectory.retry' : 'deviceDirectory.refresh')}
        disabled={state.pending !== null || state.phase === 'loading' || state.phase === 'inactive'}
        onPress={() => void session.load(ticket)}
      />
    </View>
  );
}
