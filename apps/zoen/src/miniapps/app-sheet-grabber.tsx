import { useCallback, useMemo, useRef, type ReactNode } from 'react';
import { useFocusEffect, useNavigation, usePathname } from 'expo-router';
import { CommonActions, useRoute } from 'expo-router/react-navigation';
import { StyleSheet, View } from 'react-native';
import {
  GestureDetector,
  useCompetingGestures,
  usePanGesture,
  useTapGesture,
} from 'react-native-gesture-handler';
import { SafeAreaInsetsContext, useSafeAreaInsets } from 'react-native-safe-area-context';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';

const GRABBER_HEIGHT = 48;
type Scope = { path: string };

/** Native close intent; the canvas remains full-size underneath its safe area. */
export function AppSheetGrabber({ children }: { children: ReactNode }) {
  const navigation = useNavigation('/');
  const route = useRoute();
  const path = usePathname();
  const insets = useSafeAreaInsets();
  const { colors } = useTheme();
  const active = useRef<Scope | null>(null);
  const dragging = useRef<Scope | null>(null);
  const tapping = useRef<Scope | null>(null);
  const contentInsets = useMemo(() => ({ ...insets, top: insets.top + GRABBER_HEIGHT }), [insets]);
  useFocusEffect(
    useCallback(() => {
      const scope = { path };
      active.current = scope;
      return () => {
        if (active.current === scope) active.current = null;
      };
    }, [path]),
  );
  function begin() {
    const scope = active.current;
    return scope?.path === path ? scope : null;
  }
  function close(scope: Scope | null) {
    if (!scope || active.current !== scope || scope.path !== path) return;
    const state = navigation.getState();
    if (!state) return;
    const current = state.routes[state.index ?? 0];
    if (
      !navigation.isFocused() ||
      current?.key !== route.key ||
      current.name !== 'miniapp' ||
      !navigation.canGoBack()
    )
      return;
    // The existing descendant usePreventRemove owns flush, failure and replay.
    navigation.dispatch({ ...CommonActions.goBack(), source: route.key, target: state.key });
  }
  const pan = usePanGesture({
    runOnJS: true,
    minPointers: 1,
    maxPointers: 1,
    activeOffsetY: 12,
    failOffsetY: -8,
    failOffsetX: [-12, 12],
    shouldCancelWhenOutside: false,
    onBegin: () => {
      dragging.current = begin();
    },
    onDeactivate: (event) => {
      if (!event.canceled && event.translationY >= 48 && Math.abs(event.translationX) <= 24)
        close(dragging.current);
    },
    onFinalize: () => {
      dragging.current = null;
    },
  });
  const tap = useTapGesture({
    runOnJS: true,
    maxDistance: 8,
    onBegin: () => {
      tapping.current = begin();
    },
    onDeactivate: (event) => {
      if (!event.canceled) close(tapping.current);
    },
    onFinalize: () => {
      tapping.current = null;
    },
  });
  const gesture = useCompetingGestures(pan, tap);
  const dismiss = () => close(begin());
  return (
    <View style={{ flex: 1 }}>
      <SafeAreaInsetsContext.Provider value={contentInsets}>
        {children}
      </SafeAreaInsetsContext.Provider>
      <View pointerEvents="box-none" style={StyleSheet.absoluteFill}>
        <GestureDetector gesture={gesture}>
          <View
            testID="app-sheet-grabber"
            accessible
            collapsable={false}
            accessibilityRole="button"
            accessibilityLabel={t('common.close')}
            accessibilityActions={[{ name: 'activate', label: t('common.close') }]}
            onAccessibilityAction={(event) => {
              if (event.nativeEvent.actionName === 'activate') dismiss();
            }}
            onAccessibilityTap={dismiss}
            onAccessibilityEscape={dismiss}
            style={{
              position: 'absolute',
              top: insets.top,
              alignSelf: 'center',
              width: 72,
              height: GRABBER_HEIGHT,
              alignItems: 'center',
              justifyContent: 'center',
            }}
          >
            <View
              pointerEvents="none"
              style={{ width: 36, height: 4, borderRadius: 2, backgroundColor: colors.secondary }}
            />
          </View>
        </GestureDetector>
      </View>
    </View>
  );
}
