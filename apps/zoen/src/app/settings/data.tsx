import { router } from 'expo-router';
import { DeviceDataActions } from '../../components/device-data-actions';
import { SettingsRows } from '../../components/settings-rows';
import { SettingsScreen } from '../../components/settings-screen';
import { t } from '../../i18n/index.ts';

export default function DataSettings() {
  return (
    <SettingsScreen title={t('settings.dataControls')} testID="settings-data-screen">
      <SettingsRows
        testID="settings-data-rows"
        rows={[
          {
            id: 'settings-encrypted-backup',
            title: t('settings.encryptedBackup'),
            hint: t('settings.backupHint'),
            icon: { ios: 'arrow.triangle.2.circlepath', android: 'backup', web: 'backup' },
            onPress: () => router.push('/backup'),
          },
        ]}
      />
      <DeviceDataActions />
    </SettingsScreen>
  );
}
