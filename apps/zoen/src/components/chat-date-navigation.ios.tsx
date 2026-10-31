import { BottomSheet, Host, RNHostView } from '@expo/ui';
import DateTimePicker from '@expo/ui/community/datetime-picker';
import { useFocusEffect } from 'expo-router';
import { useCallback, useState, useSyncExternalStore } from 'react';
import { AppState, ScrollView, Text, View } from 'react-native';
import { conversationScope } from '../core/conversation-scope';
import { ConversationDateJump } from '../core/conversation-date';
import { useClient } from '../core/provider';
import { t, formatLocale } from '../i18n';
import { useTheme } from '../theme';
import { IconButton, NativeButton } from './primitives';
import type { CalendarNavigationProps } from './chat-date-navigation.types';

export function ChatDateNavigation(props: CalendarNavigationProps) {
  const client = useClient();
  const scope = conversationScope(client, client.store.sessionEpoch, props.spaceId);
  const account = client.snapshot?.account;
  return scope && account ? (
    <OwnedCalendar
      key={scope}
      {...props}
      owner={scope}
      identity={account.identity_id}
      device={account.device_id}
    />
  ) : null;
}

function OwnedCalendar({
  owner,
  identity,
  device,
  spaceId,
  onOpen,
  reveal,
  onSelect,
}: CalendarNavigationProps & { owner: string; identity: string; device: string }) {
  const { store } = useClient();
  const { scheme, presentation: p } = useTheme();
  const [navigation] = useState(
    () =>
      new ConversationDateJump(
        {
          expected_identity_id: identity,
          expected_device_id: device,
          space_id: spaceId,
          thread_root_id: null,
        },
        {
          current: () =>
            AppState.currentState === 'active' &&
            !store.isSessionRetiring &&
            conversationScope(store.getSnapshot(), store.sessionEpoch, spaceId) === owner,
          read: (scope, day) => store.driver.request({ op: 'message_date_target', scope, ...day }),
          reveal,
          select: onSelect,
        },
      ),
  );
  const state = useSyncExternalStore(
    navigation.subscribe,
    navigation.getSnapshot,
    navigation.getSnapshot,
  );
  useFocusEffect(
    useCallback(() => {
      navigation.focus();
      const changed = () => {
        if (
          store.isSessionRetiring ||
          conversationScope(store.getSnapshot(), store.sessionEpoch, spaceId) !== owner
        )
          navigation.close();
      };
      const unsubscribe = store.subscribe(changed);
      const unregister = store.registerSessionCleanup(async () => navigation.close());
      const app = AppState.addEventListener('change', (state) => {
        if (state !== 'active') navigation.close();
      });
      changed();
      return () => {
        navigation.blur();
        unsubscribe();
        unregister();
        app.remove();
      };
    }, [store, owner, spaceId, navigation]),
  );
  return (
    <>
      <IconButton
        testID="chat-calendar-search"
        name={{ ios: 'calendar', android: 'calendar_month', web: 'calendar_month' }}
        label={t('chat.jumpToDate')}
        onPress={() => {
          if (navigation.open()) onOpen();
        }}
      />
      <Host
        style={{ position: 'absolute' }}
        pointerEvents="none"
        colorScheme={scheme}
        seedColor={p.list.accent}
      >
        <BottomSheet
          key={state.presentation}
          testID="chat-calendar-sheet"
          isPresented={state.presented}
          onDismiss={() => navigation.close(state.presentation)}
          snapPoints={['half', 'full']}
          containerColor={p.list.background}
        >
          <RNHostView>
            <ScrollView contentContainerStyle={{ padding: 16, paddingBottom: 32, gap: 16 }}>
              <View style={{ flexDirection: 'row', alignItems: 'center' }}>
                <Text
                  accessibilityRole="header"
                  style={{ flex: 1, fontSize: 20, fontWeight: '600', color: p.list.text }}
                >
                  {t('chat.jumpToDate')}
                </Text>
                <IconButton
                  name={{ ios: 'xmark', android: 'close', web: 'close' }}
                  label={t('common.close')}
                  onPress={() => navigation.close()}
                />
              </View>
              <DateTimePicker
                key={state.presentation}
                testID="chat-calendar-day"
                mode="date"
                display="inline"
                presentation="inline"
                minimumDate={new Date(1970, 0, 2)}
                value={new Date(state.date)}
                locale={formatLocale}
                themeVariant={scheme}
                disabled={state.pending}
                onValueChange={(_event, date) => navigation.setDate(date, state.presentation)}
              />
              {state.error ? (
                <Text
                  accessibilityRole="alert"
                  style={{
                    color: state.error === 'missing' ? p.list.secondary : p.list.destructive,
                  }}
                >
                  {t(
                    state.error === 'missing' ? 'chat.dateNoMessages' : 'chat.dateNavigationFailed',
                  )}
                </Text>
              ) : null}
              <NativeButton
                testID="chat-calendar-go"
                fullWidth
                label={t(state.pending ? 'chat.searchingMessages' : 'chat.goToDate')}
                disabled={state.pending}
                onPress={() => {
                  void navigation.jump(state);
                }}
              />
            </ScrollView>
          </RNHostView>
        </BottomSheet>
      </Host>
    </>
  );
}
