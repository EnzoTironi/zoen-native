import { Text, View } from 'react-native';
import { AppearancePicker } from '../../components/appearance-picker';
import { SettingsScreen } from '../../components/settings-screen';
import { useClient } from '../../core/provider';
import { useTheme } from '../../theme';
import { t } from '../../i18n/index.ts';

export default function GeneralSettings() {
  const { snapshot } = useClient();
  const { colors, preferenceError } = useTheme();
  const connection = snapshot?.connection;
  const status =
    connection?.state === 'online'
      ? t('settings.connected')
      : connection?.state === 'connecting'
        ? t('settings.connecting')
        : t('settings.offline');
  return (
    <SettingsScreen title={t('settings.general')} testID="settings-general-screen">
      <View style={{ gap: 12 }}>
        <Text
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}
        >
          {t('settings.appearance')}
        </Text>
        <AppearancePicker />
        {preferenceError ? (
          <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
            {preferenceError}
          </Text>
        ) : null}
      </View>
      <View testID="settings-connection" style={{ gap: 12 }}>
        <Text
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}
        >
          {t('settings.connection')}
        </Text>
        <Text selectable style={{ color: colors.secondary, fontSize: 16 }}>
          {t('settings.connectionStatus', { status })}
        </Text>
        <Text selectable style={{ color: colors.secondary, fontSize: 16 }}>
          {t('settings.pendingSync', { total: connection?.pending ?? '0' })}
        </Text>
      </View>
    </SettingsScreen>
  );
}
