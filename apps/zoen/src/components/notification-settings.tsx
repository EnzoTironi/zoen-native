import { Collapsible, Column, Host, Text as NativeText } from '@expo/ui';
import { useState } from 'react';
import { t } from '../i18n/index.ts';
import { Linking, Platform, Text, View } from 'react-native';
import { router } from 'expo-router';
import { SettingsRows } from './settings-rows';
import { SettingsScreen } from './settings-screen';
import { useNotifications } from '../notifications/provider';
import {
  notificationSettingsPresentation,
  performNotificationAction,
} from '../notifications/settings-presentation';
import { useTheme } from '../theme';
import { Icon, NativeButton } from './primitives';

export function NotificationSettings() {
  const notifications = useNotifications();
  const { presentation: p, scheme } = useTheme();
  const [details, setDetails] = useState(false);
  const [actionFailure, setActionFailure] = useState<string | null>(null);
  const native = Platform.OS === 'ios' || Platform.OS === 'android';
  const view = notificationSettingsPresentation(notifications, native);
  const registration = {
    acknowledged: 'registrationConfirmedByTheServer',
    registering: 'confirmingRegistration',
    waiting: 'waitingForServerConfirmation',
    refused: 'registrationRefusedByTheServer',
    unsupported: 'remoteNotificationsAreNotYetAvailableOnThis',
    unregistered: 'thisDeviceIsNotRegisteredYet',
  } as const;
  return (
    <SettingsScreen title={t('notifications.notifications')} testID="notification-settings">
      <View
        style={{
          backgroundColor: p.list.block,
          borderRadius: 10,
          padding: 20,
          gap: 16,
          alignItems: 'center',
        }}
      >
        <View accessible={false} accessibilityElementsHidden>
          <Icon
            name={{ ios: 'bell.badge', android: 'notifications', web: 'notifications' }}
            size={36}
            color={p.list.accent}
          />
        </View>
        <Text
          accessibilityRole="header"
          style={{ ...p.typography.title, color: p.list.text, textAlign: 'center' }}
        >
          {t(`notifications.${view.title}`)}
        </Text>
        {view.detail ? (
          <Text
            selectable
            style={{ color: p.list.secondary, fontSize: 15, lineHeight: 21, textAlign: 'center' }}
          >
            {t(`notifications.${view.detail}`)}
          </Text>
        ) : null}
        {notifications.failure || actionFailure ? (
          <Text
            accessibilityRole="alert"
            selectable
            style={{ color: p.destructive, fontSize: 15, lineHeight: 21, textAlign: 'center' }}
          >
            {actionFailure ?? notifications.failure}
          </Text>
        ) : null}
        {view.action ? (
          <NativeButton
            testID="notification-primary-action"
            label={t(`notifications.${view.label}`)}
            disabled={view.disabled}
            wrapLabel
            onPress={() => {
              setActionFailure(null);
              void performNotificationAction(view.action, {
                requestPermission: notifications.requestPermission,
                refresh: notifications.refresh,
                openSettings: () => Linking.openSettings(),
              }).catch(() => setActionFailure(t('notifications.settingsCouldNotOpen')));
            }}
          />
        ) : null}
      </View>
      <SettingsRows
        testID="notification-chat-settings"
        rows={[
          {
            id: 'notification-muted-chats',
            title: t('notifications.mutedChats'),
            icon: { ios: 'bell.slash', android: 'notifications_off', web: 'notifications_off' },
            onPress: () => router.push('/settings/muted-chats'),
          },
        ]}
      />
      <View style={{ backgroundColor: p.list.block, borderRadius: 10, padding: 16 }}>
        <Host
          matchContents={{ vertical: true }}
          style={{ width: '100%' }}
          colorScheme={scheme}
          seedColor={p.list.accent}
        >
          <Collapsible
            label={t('notifications.diagnostics')}
            labelStyle={{ color: p.list.text, fontSize: 17 }}
            isOpen={details}
            onOpenChange={setDetails}
          >
            <Column spacing={12}>
              <NativeText textStyle={{ color: p.list.secondary, fontSize: 15 }}>
                {t(`notifications.${registration[notifications.registration]}`)}
              </NativeText>
              <NativeText textStyle={{ color: p.list.secondary, fontSize: 13 }}>
                {t('notifications.registrationConfirmsThatTheServerReceivedTheRequest')}
              </NativeText>
              <NativeText textStyle={{ color: p.list.secondary, fontSize: 13 }}>
                {t('notifications.theServerOnlySendsASignalToSync')}
              </NativeText>
            </Column>
          </Collapsible>
        </Host>
      </View>
    </SettingsScreen>
  );
}
