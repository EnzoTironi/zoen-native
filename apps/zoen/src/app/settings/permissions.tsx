import { router } from 'expo-router';
import { Text, View } from 'react-native';
import { Avatar } from '../../components/primitives';
import { SettingsRows } from '../../components/settings-rows';
import { SettingsScreen } from '../../components/settings-screen';
import { useClient } from '../../core/provider';
import { useTheme } from '../../theme';
import { t } from '../../i18n/index.ts';

export default function PermissionsSettings() {
  const { snapshot, mode } = useClient();
  const { colors } = useTheme();
  const agents = snapshot?.agents ?? [];
  const canConnect = mode === 'account' && process.env.EXPO_OS === 'ios';
  return (
    <SettingsScreen title={t('settings.permissions')} testID="settings-permissions-screen">
      <SettingsRows
        testID="settings-app-permissions-rows"
        rows={[
          {
            id: 'settings-app-permissions',
            title: t('settings.appPermissions'),
            hint: t('settings.appPermissionsHint'),
            icon: { ios: 'square.grid.2x2', android: 'apps', web: 'apps' },
            onPress: () => router.push('/miniapp-permissions'),
          },
        ]}
      />
      {agents.length || canConnect ? (
        <View style={{ gap: 8 }}>
          <Text
            accessibilityRole="header"
            style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}
          >
            {t('settings.yourAgents')}
          </Text>
          <SettingsRows
            testID="settings-agent-permissions-rows"
            rows={[
              ...(canConnect
                ? [
                    {
                      id: 'settings-connect-agent',
                      title: t('agentEnrollment.connect'),
                      hint: t('agentEnrollment.connectHint'),
                      icon: {
                        ios: 'qrcode.viewfinder' as const,
                        android: 'qr_code_scanner' as const,
                        web: 'qr_code_scanner' as const,
                      },
                      onPress: () => router.push('/connect-agent'),
                    },
                  ]
                : []),
              ...agents.map((agent) => ({
                id: `settings-agent-${agent.persona.id}`,
                title: agent.persona.name,
                hint: t('settings.agentPermissions', { name: agent.persona.name }),
                leading: (
                  <Avatar
                    name={agent.persona.name}
                    color={agent.persona.tint_hex}
                    agent
                    size={36}
                  />
                ),
                onPress: () =>
                  router.push({ pathname: '/agent/[id]', params: { id: agent.persona.id } }),
              })),
            ]}
          />
        </View>
      ) : null}
    </SettingsScreen>
  );
}
