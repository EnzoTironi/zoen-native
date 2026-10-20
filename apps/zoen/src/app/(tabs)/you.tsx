import { SkyScene } from '../../components/sky-background';
import { t } from '../../i18n/index.ts';
import { Pressable, ScrollView, Text, View } from 'react-native';
import { useDialogs } from '../../components/dialogs';
import { router } from 'expo-router';
import { useProfilePhoto } from '../../profile/use-profile-photo';
import { useClient } from '../../core/provider';
import { useTheme } from '../../theme';
import { Avatar, NativeButton, Surface } from '../../components/primitives';
import { DeviceDataActions } from '../../components/device-data-actions';
import { IntegrityWarnings } from '../../integrity/integrity-warning';
import { AppearancePicker } from '../../components/appearance-picker';

function YouContent() {
  const { snapshot, store, mode } = useClient();
  const { colors, preferenceError } = useTheme();
  const { confirm } = useDialogs();
  const me = snapshot?.me;
  const photo = useProfilePhoto(me?.id, me?.photo_sha256, me?.photo_ready ?? false);
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{ padding: 24, gap: 24, backgroundColor: 'transparent' }}
    >
      <Text
        accessibilityRole="header"
        style={{ fontSize: 34, fontWeight: '700', color: colors.text }}
      >
        {t('settings.you')}
      </Text>
      <Surface>
        <View style={{ flexDirection: 'row', gap: 18, alignItems: 'center' }}>
          <Avatar uri={photo} name={me?.name ?? t('settings.you')} color={me?.tint_hex} size={66} />
          <View style={{ flex: 1, gap: 5 }}>
            <Text style={{ fontSize: 23, fontWeight: '600', color: colors.text }}>
              {me?.name ?? t('settings.yourAccount')}
            </Text>
            <Text style={{ fontSize: 15, color: colors.secondary }}>@{me?.handle}</Text>
          </View>
        </View>
        {me?.bio ? <Text style={{ color: colors.secondary, lineHeight: 23 }}>{me.bio}</Text> : null}
        <NativeButton
          secondary
          label={t('settings.editMyProfile')}
          onPress={() => router.push('/edit-profile')}
        />
      </Surface>
      <IntegrityWarnings />
      <Surface>
        <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
          {t('settings.appearance')}
        </Text>
        <AppearancePicker />
      </Surface>
      {preferenceError ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {preferenceError}
        </Text>
      ) : null}
      {snapshot?.agents.length ? (
        <Surface>
          <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
            {t('settings.yourAgents')}
          </Text>
          {snapshot?.agents.map((agent) => (
            <Pressable
              accessibilityRole="button"
              accessibilityLabel={t('settings.agentPermissions', { name: agent.persona.name })}
              onPress={() =>
                router.push({ pathname: '/agent/[id]', params: { id: agent.persona.id } })
              }
              key={agent.persona.id}
              style={{ flexDirection: 'row', gap: 12, alignItems: 'center', paddingVertical: 8 }}
            >
              <Avatar name={agent.persona.name} color={agent.persona.tint_hex} agent size={40} />
              <View style={{ flex: 1 }}>
                <Text style={{ color: colors.text, fontSize: 16, fontWeight: '600' }}>
                  {agent.persona.name}
                </Text>
              </View>
            </Pressable>
          ))}
        </Surface>
      ) : null}
      <Surface>
        <Text style={{ color: colors.text, fontWeight: '600', fontSize: 18 }}>
          {t('settings.connection')}
        </Text>
        <NativeButton
          secondary
          label={t('settings.linkedDevices')}
          onPress={() => router.push('/devices')}
        />
        <NativeButton
          secondary
          label={t('settings.encryptedBackup')}
          onPress={() => router.push('/backup')}
        />
        <NativeButton
          secondary
          label={t('settings.notifications')}
          onPress={() => router.push('/notifications')}
        />
        <Text style={{ color: colors.secondary }}>
          {t('settings.connectionStatus', {
            status:
              snapshot?.connection.state === 'online'
                ? t('settings.connected')
                : snapshot?.connection.state === 'connecting'
                  ? t('settings.connecting')
                  : t('settings.offline'),
          })}
        </Text>
        <Text style={{ color: colors.secondary }}>
          {t('settings.pendingSync', { total: snapshot?.connection.pending ?? '0' })}
        </Text>
      </Surface>
      <DeviceDataActions />
      {mode === 'demo' ? (
        <NativeButton
          label={t('settings.useYourOwnAccount')}
          onPress={() => {
            void confirm({
              title: t('settings.leaveTheDemo'),
              message: t('settings.demoChatsAreKeptSeparateFromYourAccount'),
            }).then((accepted) => {
              if (accepted) void store.open('account');
            });
          }}
        />
      ) : null}
    </ScrollView>
  );
}

export default function You() {
  return (
    <SkyScene tabScene>
      <YouContent />
    </SkyScene>
  );
}
