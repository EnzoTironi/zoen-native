// Telegram-iOS f1dd7a2 NotificationMuteSettingsController and
// PeerInfoScreen.openCustomMute -> ChatTimerScreen(.mute): date/time + Apply.
import { BottomSheet, Host, RNHostView } from '@expo/ui';
import DateTimePicker from '@expo/ui/community/datetime-picker';
import { useState } from 'react';
import { Platform, ScrollView, Text, View } from 'react-native';
import { IconButton, NativeButton } from '../components/primitives';
import { SettingsRows } from '../components/settings-rows';
import { t, formatLocale } from '../i18n';
import { useTheme } from '../theme';
import { muteDate, muteSummary } from './mute-presentation';
import type { MuteChoice, MutedChat } from './muted-chats';

export function MuteDurationSheet({
  chat,
  clock,
  busy,
  onDismiss,
  onChange,
  onOpen,
  onUnmute,
}: {
  chat: MutedChat | null;
  clock: string | null;
  busy: boolean;
  onDismiss(): void;
  onChange(choice: MuteChoice): void;
  onOpen?: () => void;
  onUnmute(): void;
}) {
  const { presentation: p, scheme } = useTheme();
  const [custom, setCustom] = useState(false);
  const [date, setDate] = useState(() => new Date(0));
  const minimum = muteDate(clock);
  return (
    <Host
      style={{ position: 'absolute' }}
      pointerEvents="none"
      colorScheme={scheme}
      seedColor={p.list.accent}
    >
      <BottomSheet
        testID="muted-chat-sheet"
        isPresented={Boolean(chat)}
        onDismiss={onDismiss}
        snapPoints={custom ? ['full'] : ['half', 'full']}
        containerColor={p.list.background}
      >
        <RNHostView>
          <ScrollView
            style={{ flex: 1 }}
            contentInsetAdjustmentBehavior="automatic"
            contentContainerStyle={{ gap: 20, paddingVertical: 16, paddingBottom: 32 }}
          >
            {chat ? (
              <>
                <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
                  <Text
                    accessibilityRole="header"
                    selectable
                    style={{ flex: 1, color: p.list.text, fontSize: 24, fontWeight: '600' }}
                  >
                    {chat.title || t('notifications.unnamedChat')}
                  </Text>
                  <IconButton
                    name={{ ios: 'xmark', android: 'close', web: 'close' }}
                    label={t('common.close')}
                    onPress={onDismiss}
                  />
                </View>
                <Text style={{ color: p.list.secondary, fontSize: 15 }}>
                  {muteSummary(chat.muted_until_ms)}
                </Text>
                {custom ? (
                  <>
                    <DateTimePicker
                      testID="mute-until-picker"
                      value={date}
                      minimumDate={minimum ?? undefined}
                      mode={Platform.OS === 'android' ? 'date' : 'datetime'}
                      presentation="inline"
                      display="spinner"
                      locale={formatLocale}
                      themeVariant={scheme}
                      disabled={busy}
                      onValueChange={(_event, value) => setDate(value)}
                    />
                    {Platform.OS === 'android' ? (
                      <DateTimePicker
                        testID="mute-until-time-picker"
                        value={date}
                        mode="time"
                        presentation="inline"
                        display="spinner"
                        onValueChange={(_event, value) => setDate(value)}
                      />
                    ) : null}
                    <NativeButton
                      testID="mute-until-apply"
                      wrapLabel
                      label={t('notifications.muteUntil', {
                        date: date.toLocaleString(formatLocale, {
                          dateStyle: 'short',
                          timeStyle: 'short',
                        }),
                      })}
                      disabled={busy || !minimum || date.getTime() <= minimum.getTime()}
                      onPress={() => onChange({ kind: 'until', until_ms: String(date.getTime()) })}
                    />
                    <NativeButton
                      secondary
                      label={t('common.cancel')}
                      disabled={busy}
                      onPress={() => setCustom(false)}
                    />
                  </>
                ) : (
                  <>
                    {onOpen ? (
                      <NativeButton
                        testID="muted-chat-open"
                        label={t('notifications.openChat')}
                        secondary
                        disabled={busy}
                        onPress={onOpen}
                      />
                    ) : null}
                    <NativeButton
                      testID="muted-chat-unmute"
                      label={t('notifications.reenable')}
                      disabled={busy || chat.muted_until_ms === null}
                      onPress={onUnmute}
                    />
                    <SettingsRows
                      testID="mute-duration-options"
                      rows={[
                        {
                          id: 'mute-one-hour',
                          title: t('notifications.muteOneHour'),
                          disabled: busy || !minimum,
                          onPress: () => onChange({ kind: 'for', duration_seconds: 3600 }),
                        },
                        {
                          id: 'mute-two-days',
                          title: t('notifications.muteTwoDays'),
                          disabled: busy || !minimum,
                          onPress: () => onChange({ kind: 'for', duration_seconds: 172800 }),
                        },
                        {
                          id: 'mute-custom',
                          title: t('notifications.muteCustom'),
                          disabled: busy || !minimum,
                          onPress: () => {
                            if (minimum) {
                              setDate(new Date(minimum.getTime() + 3600000));
                              setCustom(true);
                            }
                          },
                        },
                        {
                          id: 'mute-forever',
                          title: t('notifications.muteForever'),
                          disabled: busy,
                          onPress: () => onChange({ kind: 'forever' }),
                        },
                      ]}
                    />
                  </>
                )}
              </>
            ) : null}
          </ScrollView>
        </RNHostView>
      </BottomSheet>
    </Host>
  );
}
