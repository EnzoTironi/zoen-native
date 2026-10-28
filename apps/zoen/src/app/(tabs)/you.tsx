import { SkyScene } from '../../components/sky-background';
import { t } from '../../i18n/index.ts';
import { ScrollView, Text, View } from 'react-native';
import { useDialogs } from '../../components/dialogs';
import { router } from 'expo-router';
import { useProfilePhoto } from '../../profile/use-profile-photo';
import { useClient } from '../../core/provider';
import { useTheme } from '../../theme';
import { NativeButton } from '../../components/primitives';
import { IntegrityWarnings } from '../../integrity/integrity-warning';
import { SettingsRows } from '../../components/settings-rows';
import { ProfileHeader } from '../../profile/profile-header';
import { ProfileInfoRow, ProfileSection } from '../../profile/profile-section';

function YouContent() {
  const { snapshot, store, mode } = useClient();
  const { colors, presentation: p, preferenceError } = useTheme();
  const { confirm } = useDialogs();
  const me = snapshot?.me;
  const photo = useProfilePhoto(me?.id, me?.photo_sha256, me?.photo_ready ?? false);
  return (
    <ScrollView
      testID="you-settings-screen"
      style={{ flex: 1, backgroundColor: p.list.background }}
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        padding: 16,
        paddingTop: 18,
        paddingBottom: 48,
        gap: 32,
        maxWidth: 640,
        width: '100%',
        alignSelf: 'center',
        backgroundColor: 'transparent',
      }}
    >
      <ProfileHeader
        top={4}
        name={me?.name ?? t('settings.yourAccount')}
        subtitle={me?.handle ? `@${me.handle}` : undefined}
        photo={photo}
        tint={me?.tint_hex}
        onAvatarPress={() => router.push('/edit-profile')}
        avatarLabel={t('settings.editMyProfile')}
      />
      <View style={{ gap: 16 }}>
        {me?.bio ? (
          <ProfileSection>
            <ProfileInfoRow label={t('profile.about')} value={me.bio} />
          </ProfileSection>
        ) : null}
        <SettingsRows
          testID="profile-settings-rows"
          rows={[
            ...(mode === 'account'
              ? [
                  {
                    id: 'settings-saved-messages',
                    iconColor: '#007AFF',
                    title: t('settings.savedMessages'),
                    icon: {
                      ios: 'bookmark.fill' as const,
                      android: 'bookmark' as const,
                      web: 'bookmark' as const,
                    },
                    onPress: () => router.push('/saved-messages'),
                  },
                ]
              : []),
            {
              id: 'settings-edit-profile',
              iconColor: '#007AFF',
              title: t('settings.editMyProfile'),
              icon: { ios: 'pencil', android: 'edit', web: 'edit' },
              onPress: () => router.push('/edit-profile'),
            },
          ]}
        />
      </View>
      <IntegrityWarnings />
      <View style={{ gap: 8 }}>
        <Text
          accessibilityRole="header"
          style={{
            color: p.list.section,
            fontSize: 13,
            marginLeft: 16,
            textTransform: 'uppercase',
          }}
        >
          {t('settings.settings')}
        </Text>
        <SettingsRows
          testID="you-settings-rows"
          rows={[
            ...(mode === 'account'
              ? [
                  {
                    id: 'settings-account-budget',
                    title: t('ownerPolicy.title'),
                    iconColor: '#34AADC',
                    icon: {
                      ios: 'creditcard' as const,
                      android: 'account_balance_wallet' as const,
                      web: 'account_balance_wallet' as const,
                    },
                    onPress: () => router.push('/account-budget'),
                  },
                ]
              : []),
            {
              id: 'settings-appearance',
              iconColor: '#5856D6',
              title: t('settings.appearance'),
              icon: { ios: 'paintbrush', android: 'palette', web: 'palette' },
              onPress: () => router.push('/settings/appearance'),
            },
            {
              id: 'settings-general',
              iconColor: '#8E8E93',
              title: t('settings.general'),
              hint: t('settings.generalHint'),
              icon: { ios: 'gearshape', android: 'settings', web: 'settings' },
              onPress: () => router.push('/settings/general'),
            },
            {
              id: 'settings-permissions',
              iconColor: '#AF52DE',
              title: t('settings.permissions'),
              hint: t('settings.permissionsHint'),
              icon: { ios: 'hand.raised', android: 'back_hand', web: 'back_hand' },
              onPress: () => router.push('/settings/permissions'),
            },
            {
              id: 'settings-notifications',
              iconColor: '#FF3B30',
              title: t('settings.notifications'),
              hint: t('settings.notificationsHint'),
              icon: { ios: 'bell', android: 'notifications', web: 'notifications' },
              onPress: () => router.push('/notifications'),
            },
            {
              id: 'settings-devices',
              iconColor: '#34AADC',
              title: t('settings.devices'),
              hint: t('settings.devicesHint'),
              icon: { ios: 'laptopcomputer.and.iphone', android: 'devices', web: 'devices' },
              onPress: () => router.push('/devices'),
            },
            {
              id: 'settings-data-controls',
              iconColor: '#FF9500',
              title: t('settings.dataControls'),
              hint: t('settings.dataControlsHint'),
              icon: { ios: 'lock', android: 'lock', web: 'lock' },
              onPress: () => router.push('/settings/data'),
            },
          ]}
        />
      </View>
      {mode === 'account' ? (
        <View style={{ gap: 8 }}>
          <Text
            accessibilityRole="header"
            style={{
              color: p.list.section,
              fontSize: 13,
              marginLeft: 16,
              textTransform: 'uppercase',
            }}
          >
            {t('settings.privacyAndSafety')}
          </Text>
          <SettingsRows
            testID="you-privacy-rows"
            rows={[
              {
                id: 'settings-blocked-people',
                iconColor: '#FF3B30',
                title: t('settings.blockedPeople'),
                hint: t('settings.blockedPeopleDescription'),
                icon: { ios: 'hand.raised.slash', android: 'block', web: 'block' },
                onPress: () => router.push('/settings/blocked-people'),
              },
            ]}
          />
        </View>
      ) : null}
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
