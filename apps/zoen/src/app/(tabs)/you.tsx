import { SkyScene } from '../../components/sky-background';
import { t } from '../../i18n/index.ts';
import { ScrollView, Text, View } from 'react-native';
import { useDialogs } from '../../components/dialogs';
import { router } from 'expo-router';
import { useProfilePhoto } from '../../profile/use-profile-photo';
import { useClient } from '../../core/provider';
import { useTheme } from '../../theme';
import { Avatar, NativeButton } from '../../components/primitives';
import { IntegrityWarnings } from '../../integrity/integrity-warning';
import { SettingsRows } from '../../components/settings-rows';

function YouContent() {
  const { snapshot, store, mode } = useClient();
  const { colors, preferenceError } = useTheme();
  const { confirm } = useDialogs();
  const me = snapshot?.me;
  const photo = useProfilePhoto(me?.id, me?.photo_sha256, me?.photo_ready ?? false);
  return (
    <ScrollView
      testID="you-settings-screen"
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        padding: 24,
        paddingBottom: 48,
        gap: 28,
        maxWidth: 640,
        width: '100%',
        alignSelf: 'center',
        backgroundColor: 'transparent',
      }}
    >
      <Text
        accessibilityRole="header"
        style={{ fontSize: 34, fontWeight: '700', color: colors.text }}
      >
        {t('settings.you')}
      </Text>
      <View style={{ gap: 16 }}>
        <View style={{ flexDirection: 'row', gap: 18, alignItems: 'center' }}>
          <Avatar uri={photo} name={me?.name ?? t('settings.you')} color={me?.tint_hex} size={66} />
          <View style={{ flex: 1, minWidth: 0, gap: 5 }}>
            <Text selectable style={{ fontSize: 23, fontWeight: '600', color: colors.text }}>
              {me?.name ?? t('settings.yourAccount')}
            </Text>
            {me?.handle ? (
              <Text selectable style={{ fontSize: 15, color: colors.secondary }}>
                @{me.handle}
              </Text>
            ) : null}
          </View>
        </View>
        {me?.bio ? (
          <Text selectable style={{ color: colors.secondary, lineHeight: 23 }}>
            {me.bio}
          </Text>
        ) : null}
        <NativeButton
          testID="settings-edit-profile"
          secondary
          label={t('settings.editMyProfile')}
          onPress={() => router.push('/edit-profile')}
        />
      </View>
      <IntegrityWarnings />
      <View style={{ gap: 8 }}>
        <Text
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}
        >
          {t('settings.settings')}
        </Text>
        <SettingsRows
          testID="you-settings-rows"
          rows={[
            {
              id: 'settings-general',
              title: t('settings.general'),
              hint: t('settings.generalHint'),
              icon: { ios: 'gearshape', android: 'settings', web: 'settings' },
              onPress: () => router.push('/settings/general'),
            },
            {
              id: 'settings-permissions',
              title: t('settings.permissions'),
              hint: t('settings.permissionsHint'),
              icon: { ios: 'hand.raised', android: 'back_hand', web: 'back_hand' },
              onPress: () => router.push('/settings/permissions'),
            },
            {
              id: 'settings-notifications',
              title: t('settings.notifications'),
              hint: t('settings.notificationsHint'),
              icon: { ios: 'bell', android: 'notifications', web: 'notifications' },
              onPress: () => router.push('/notifications'),
            },
            {
              id: 'settings-devices',
              title: t('settings.devices'),
              hint: t('settings.devicesHint'),
              icon: { ios: 'laptopcomputer.and.iphone', android: 'devices', web: 'devices' },
              onPress: () => router.push('/devices'),
            },
            {
              id: 'settings-data-controls',
              title: t('settings.dataControls'),
              hint: t('settings.dataControlsHint'),
              icon: { ios: 'lock', android: 'lock', web: 'lock' },
              onPress: () => router.push('/settings/data'),
            },
          ]}
        />
      </View>
      {preferenceError ? (
        <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
          {preferenceError}
        </Text>
      ) : null}
      {mode === 'demo' ? (
        <NativeButton
          testID="settings-use-account"
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
