import { t } from '../i18n/index.ts';
import { Linking, Platform, ScrollView, Text, View } from 'react-native';
import { Stack } from 'expo-router';
import { useNotifications } from '../notifications/provider';
import { useTheme } from '../theme';
import { Icon, NativeButton, Surface } from './primitives';

export function NotificationSettings() {
  const notifications = useNotifications();
  const { colors } = useTheme();
  const status =
    notifications.registration === 'acknowledged'
      ? t('notifications.registrationConfirmedByTheServer')
      : notifications.registration === 'registering'
        ? t('notifications.confirmingRegistration')
        : notifications.registration === 'waiting'
          ? t('notifications.waitingForServerConfirmation')
          : notifications.registration === 'refused'
            ? t('notifications.registrationRefusedByTheServer')
            : notifications.registration === 'unsupported'
              ? t('notifications.remoteNotificationsAreNotYetAvailableOnThis')
              : t('notifications.thisDeviceIsNotRegisteredYet');
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        padding: 24,
        gap: 24,
        maxWidth: 720,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen options={{ title: t('notifications.notifications') }} />
      <Surface>
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: 14 }}>
          <Icon
            name={{ ios: 'bell', android: 'notifications', web: 'notifications' }}
            size={28}
            color={colors.accent}
          />
          <Text
            accessibilityRole="header"
            style={{ color: colors.text, fontSize: 22, fontWeight: '600' }}
          >
            {t('notifications.notificationsUnderYourControl')}
          </Text>
        </View>
        <Text style={{ color: colors.secondary, fontSize: 16, lineHeight: 25 }}>
          {t('notifications.theServerOnlySendsASignalToSync')}
        </Text>
      </Surface>
      <Surface>
        <Text
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}
        >
          {t('notifications.devicePermission')}
        </Text>
        <Text style={{ color: colors.secondary, lineHeight: 23 }}>
          {notifications.permission === 'allowed'
            ? t('notifications.allowed')
            : notifications.permission === 'denied'
              ? t('notifications.disabledInSettings')
              : notifications.permission === 'undetermined'
                ? t('notifications.youHaveNotChosenYet')
                : Platform.OS === 'web'
                  ? t('notifications.unavailableInThisBrowser')
                  : t('notifications.checking')}
        </Text>
        {Platform.OS === 'ios' || Platform.OS === 'android' ? (
          <NativeButton
            label={
              notifications.busy
                ? t('notifications.settingUp')
                : notifications.permission === 'denied'
                  ? t('notifications.openSettings')
                  : notifications.permission === 'allowed'
                    ? t('notifications.checkAgain')
                    : t('notifications.allowNotifications')
            }
            disabled={notifications.busy}
            onPress={() => {
              if (notifications.permission === 'denied') void Linking.openSettings();
              else if (notifications.permission === 'allowed') void notifications.refresh();
              else void notifications.requestPermission();
            }}
          />
        ) : (
          <Text style={{ color: colors.secondary, lineHeight: 23 }}>
            {t('notifications.notificationsAreNotYetAvailableInThisBrowser')}
          </Text>
        )}
      </Surface>
      <Surface>
        <Text
          accessibilityRole="header"
          style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}
        >
          {status}
        </Text>
        <Text style={{ color: colors.secondary, lineHeight: 23 }}>
          {t('notifications.registrationConfirmsThatTheServerReceivedTheRequest')}
        </Text>
        {notifications.failure ? (
          <Text
            accessibilityRole="alert"
            selectable
            style={{ color: colors.danger, lineHeight: 22 }}
          >
            {notifications.failure}
          </Text>
        ) : null}
      </Surface>
    </ScrollView>
  );
}
