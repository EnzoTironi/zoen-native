import { useCallback, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useFocusEffect } from 'expo-router';
import { AppState, Platform, useWindowDimensions, type View } from 'react-native';
import { SafeAreaInsetsContext, useSafeAreaInsets } from 'react-native-safe-area-context';
import Animated, {
  measure,
  useAnimatedReaction,
  useAnimatedRef,
  useAnimatedStyle,
  useSharedValue,
} from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';
import { useKeyboardMotion } from '../components/keyboard-motion';

/** Resize the real native frame; HTML, React and canvas Apps keep their own scroll/input policy. */
export function AppKeyboardViewport({
  children,
  enabled,
  scope,
}: {
  children: ReactNode;
  enabled: boolean;
  scope: string;
}) {
  return enabled && Platform.OS === 'ios' ? (
    <NativeAppKeyboardViewport scope={scope}>{children}</NativeAppKeyboardViewport>
  ) : (
    children
  );
}

function NativeAppKeyboardViewport({ children, scope }: { children: ReactNode; scope: string }) {
  const insets = useSafeAreaInsets();
  const window = useWindowDimensions();
  const keyboard = useKeyboardMotion();
  const frame = useAnimatedRef<View>();
  const owner = useMemo(() => ({ scope }), [scope]);
  const lifetime = useRef({ owner, epoch: 0, focused: false, foreground: false, mounted: false });
  const live = useSharedValue(false);
  const epoch = useSharedValue(0);
  const layout = useSharedValue(0);
  const covered = useSharedValue(0);
  const [bottomCovered, setBottomCovered] = useState(0);
  const publish = useCallback((observedEpoch: number, pixels: number) => {
    const current = lifetime.current;
    if (
      current.epoch !== observedEpoch ||
      !current.mounted ||
      !current.focused ||
      !current.foreground ||
      !Number.isFinite(pixels) ||
      pixels < 0
    )
      return;
    setBottomCovered((previous) => (previous === pixels ? previous : pixels));
  }, []);
  const update = useCallback(
    (reset: boolean) => {
      const current = lifetime.current;
      current.epoch += 1;
      epoch.set(current.epoch);
      live.set(current.mounted && current.focused && current.foreground);
      if (reset && current.mounted) setBottomCovered(0);
    },
    [epoch, live],
  );
  useLayoutEffect(() => {
    const current = lifetime.current;
    current.owner = owner;
    current.mounted = true;
    current.focused = false;
    current.foreground = AppState.currentState === 'active';
    update(true);
    const background = AppState.addEventListener('change', (state) => {
      if (lifetime.current.owner !== owner || !lifetime.current.mounted) return;
      lifetime.current.foreground = state === 'active';
      update(true);
    });
    return () => {
      background.remove();
      if (current.owner !== owner) return;
      current.mounted = false;
      update(false);
    };
  }, [owner, update]);
  useFocusEffect(
    useCallback(() => {
      if (lifetime.current.owner !== owner || !lifetime.current.mounted) return;
      lifetime.current.focused = true;
      update(true);
      return () => {
        if (lifetime.current.owner !== owner) return;
        lifetime.current.focused = false;
        update(true);
      };
    }, [owner, update]),
  );
  useAnimatedReaction(
    () => ({
      height: keyboard.height.get(),
      revision: layout.get(),
      active: live.get(),
      epoch: epoch.get(),
      width: window.width,
      screenHeight: window.height,
      bottom: insets.bottom,
    }),
    (current) => {
      if (!current.active) {
        covered.set(0);
        return;
      }
      if (!Number.isFinite(current.height) || current.height < 0) return;
      let overlap = 0;
      if (current.height > 0) {
        const bounds = measure(frame);
        // Native rotations may briefly report null/zero or the preceding window.
        // Keep the last positive viewport until the current frame can be measured.
        if (
          !bounds ||
          !Number.isFinite(bounds.pageY) ||
          !Number.isFinite(bounds.height) ||
          !Number.isFinite(bounds.width) ||
          !Number.isFinite(current.width) ||
          current.width <= 0 ||
          !Number.isFinite(current.screenHeight) ||
          current.screenHeight <= 0 ||
          current.height > current.screenHeight ||
          bounds.height <= 0 ||
          bounds.width <= 0 ||
          bounds.height > current.screenHeight + 1 ||
          bounds.width > current.width + 1
        )
          return;
        const keyboardTop = current.screenHeight - current.height;
        overlap = Math.min(
          bounds.height - 1,
          Math.max(0, bounds.pageY + bounds.height - keyboardTop),
        );
      }
      covered.set(overlap);
      // Only the physical home-indicator area is removed from the App's inset.
      // Geometry still comes from each real child onLayout through its existing bridge.
      scheduleOnRN(publish, current.epoch, Math.min(current.bottom, overlap));
    },
  );
  const style = useAnimatedStyle(() => ({ paddingBottom: covered.get() }));
  const safeInsets = useMemo(
    () =>
      bottomCovered === 0
        ? insets
        : { ...insets, bottom: Math.max(0, insets.bottom - bottomCovered) },
    [insets, bottomCovered],
  );
  return (
    <Animated.View
      ref={frame}
      collapsable={false}
      testID="app-keyboard-viewport"
      style={[{ flex: 1, minHeight: 0 }, style]}
      onLayout={({ nativeEvent }) => {
        if (lifetime.current.owner !== owner || !lifetime.current.mounted) return;
        const { width, height } = nativeEvent.layout;
        if (Number.isFinite(width) && width > 0 && Number.isFinite(height) && height > 0)
          layout.set((previous) => previous + 1);
      }}
    >
      <SafeAreaInsetsContext.Provider value={safeInsets}>{children}</SafeAreaInsetsContext.Provider>
    </Animated.View>
  );
}
