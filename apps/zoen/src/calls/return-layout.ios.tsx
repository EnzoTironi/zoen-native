import { useSyncExternalStore, type ReactNode } from 'react';
import { Pressable, Text, View } from 'react-native';
import { SafeAreaProvider, useSafeAreaInsets } from 'react-native-safe-area-context';
import { SymbolView } from 'expo-symbols';
import Animated, { FadeIn, ReduceMotion, useReducedMotion } from 'react-native-reanimated';
import { useNativeCalls } from './provider';
import { callElapsed } from './presentation.ts';
import { useTheme } from '../theme';
import { useClient } from '../core/provider';
import { t } from '../i18n/index.ts';

const clockSnapshot = () => Math.floor(Date.now() / 1000) * 1000;
const withoutClock = () => () => {};
const subscribeClock = (changed: () => void) => {
  const timer = setInterval(changed, 1000);
  return () => clearInterval(timer);
};

/** A measured layout slot, never a touch-blocking window over the chat HUD. */
export function CallReturnLayout({ children }: { children: ReactNode }) {
  const { activeCall, showActiveCall } = useNativeCalls();
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const reducedMotion = useReducedMotion();
  const { mode } = useClient();
  const visible = mode === 'account' && activeCall?.minimized === true;
  const now = useSyncExternalStore(
    visible && activeCall?.state === 'connected' && activeCall.connected_at_ms !== null
      ? subscribeClock
      : withoutClock,
    clockSnapshot,
    clockSnapshot,
  );
  const elapsed = activeCall ? callElapsed(activeCall.connected_at_ms, now) : null;
  const detail =
    activeCall?.state === 'reconnecting'
      ? t('chat.reconnectingCall')
      : activeCall?.state === 'connected'
        ? elapsed
        : activeCall?.state === 'calling'
          ? t('chat.callingPeer')
          : t('chat.connectingCall');
  return (
    <View style={{ flex: 1 }}>
      {visible && (
        <Animated.View
          entering={FadeIn.duration(reducedMotion ? 150 : 200).reduceMotion(ReduceMotion.Never)}
          style={{ paddingTop: insets.top, backgroundColor: colors.accent }}
        >
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={t('chat.returnToCall')}
            accessibilityHint={t('chat.returnToCallHint')}
            onPress={() => {
              void showActiveCall().catch(() => {});
            }}
            style={({ pressed }) => ({
              minHeight: 44,
              paddingHorizontal: 20,
              paddingVertical: 6,
              flexDirection: 'row',
              alignItems: 'center',
              justifyContent: 'center',
              gap: 10,
              opacity: pressed ? 0.75 : 1,
            })}
          >
            <SymbolView name="phone.fill" size={17} tintColor={colors.onAccent} />
            <Text
              style={{ color: colors.onAccent, fontSize: 13, fontWeight: '600' }}
              numberOfLines={1}
            >
              {t('chat.returnToCall')}
              {detail ? ` · ${detail}` : ''}
            </Text>
            <SymbolView name="chevron.up" size={12} tintColor={colors.onAccent} />
          </Pressable>
        </Animated.View>
      )}
      {/* Its measured frame excludes the reserved banner. Native stacks and RN
          consumers receive the real remaining safe area, including the keyboard. */}
      <SafeAreaProvider style={{ flex: 1 }}>{children}</SafeAreaProvider>
    </View>
  );
}
