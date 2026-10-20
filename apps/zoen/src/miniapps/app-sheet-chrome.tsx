import { useCallback, useMemo, useRef, useState, type ReactNode } from 'react';
import { useFocusEffect, useNavigation, usePathname } from 'expo-router';
import { CommonActions, useRoute } from 'expo-router/react-navigation';
import { StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { t } from '../i18n/index.ts';
import { ComposerAction } from '../components/composer-action';
import { AppSheetCloseButtonContext } from './app-sheet-context';

type Scope = { path: string; routeKey: string };

/** Floating native close intent; the App keeps its full frame and real safe-area insets. */
export function AppSheetChrome({ children }: { children: ReactNode }) {
  const navigation = useNavigation('/');
  const route = useRoute();
  const path = usePathname();
  // Nested Details and tool screens own their native Back and header actions.
  const showClose = /^\/miniapp\/[^/]+\/?$/.test(path);
  const insets = useSafeAreaInsets();
  const active = useRef<Scope | null>(null);
  const [scope, setScope] = useState<Scope | null>(null);
  const closeButton = useMemo(
    () => ({ top: insets.top + 8, right: insets.right + 12, width: 44, height: 44 }),
    [insets.top, insets.right],
  );
  useFocusEffect(
    useCallback(() => {
      const current = { path, routeKey: route.key };
      active.current = current;
      setScope(current);
      return () => {
        if (active.current === current) active.current = null;
      };
    }, [path, route.key]),
  );
  function close(observed: Scope | null) {
    if (!showClose || !observed || active.current !== observed || observed.path !== path) return;
    const state = navigation.getState();
    if (!state) return;
    const current = state.routes[state.index ?? 0];
    if (
      !navigation.isFocused() ||
      observed.routeKey !== route.key ||
      current?.key !== route.key ||
      current.name !== 'miniapp' ||
      !navigation.canGoBack()
    )
      return;
    // The existing descendant usePreventRemove owns flush, denial and replay.
    navigation.dispatch({ ...CommonActions.goBack(), source: route.key, target: state.key });
  }
  const dismiss = () => close(scope);
  return (
    <View style={{ flex: 1 }} onAccessibilityEscape={showClose ? dismiss : undefined}>
      <AppSheetCloseButtonContext.Provider value={showClose ? closeButton : null}>
        {children}
      </AppSheetCloseButtonContext.Provider>
      {showClose ? (
        <View pointerEvents="box-none" style={StyleSheet.absoluteFill}>
          <View style={{ position: 'absolute', top: closeButton.top, right: closeButton.right }}>
            <ComposerAction
              testID="app-sheet-close"
              name="xmark"
              label={t('common.close')}
              size={44}
              disabled={!scope}
              onPress={dismiss}
            />
          </View>
        </View>
      ) : null}
    </View>
  );
}
