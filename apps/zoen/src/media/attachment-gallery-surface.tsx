import { useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import {
  View,
  type NativeScrollEvent,
  type NativeSyntheticEvent,
  type ScrollViewInstance,
} from 'react-native';
import Animated, {
  Easing,
  useAnimatedScrollHandler,
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from 'react-native-reanimated';

/** The gallery's vertical native scroll owns dismissal; the horizontal pager and zoom own their axes. */
export function AttachmentGallerySurface({
  identity,
  disabled,
  reducedMotion,
  controlsHidden,
  isCurrent,
  onClose,
  children,
  overlay,
}: {
  identity: string;
  disabled: boolean;
  reducedMotion: boolean;
  controlsHidden: boolean;
  isCurrent(): boolean;
  onClose(): void;
  children: ReactNode;
  overlay: ReactNode;
}) {
  const scroll = useRef<ScrollViewInstance | null>(null);
  const [geometry, setGeometry] = useState({ width: 0, height: 0, epoch: 0 });
  const liveGeometry = useRef(geometry);
  const binding = useMemo(() => ({ identity, geometry, disabled }), [identity, geometry, disabled]);
  const committed = useRef<typeof binding | null>(null);
  const dragging = useRef<typeof binding | null>(null);
  const dismissing = useRef(false);
  const distance = useSharedValue(0);
  const reveal = useSharedValue(controlsHidden ? 0 : 1);
  const uiBinding = useSharedValue('');
  const token = JSON.stringify([identity, geometry.epoch, disabled]);
  const owner = useMemo(() => ({ identity }), [identity]);
  const mountedOwner = useRef<typeof owner | null>(owner);
  useLayoutEffect(() => {
    mountedOwner.current = owner;
    return () => {
      if (mountedOwner.current === owner) mountedOwner.current = null;
    };
  }, [owner]);
  useLayoutEffect(() => {
    reveal.set(
      withTiming(controlsHidden ? 0 : 1, {
        duration: reducedMotion ? 0 : 200,
        easing: Easing.linear,
      }),
    );
  }, [controlsHidden, reducedMotion, reveal]);
  useLayoutEffect(() => {
    committed.current = binding;
    uiBinding.set(token);
    dragging.current = null;
    dismissing.current = false;
    distance.set(0);
    if (geometry.height > 0) scroll.current?.scrollTo({ y: geometry.height, animated: false });
    return () => {
      if (committed.current === binding) {
        committed.current = null;
        uiBinding.set('');
      }
    };
  }, [binding, distance, geometry.height, token, uiBinding]);
  const admitted = () =>
    committed.current === binding && liveGeometry.current === geometry && !disabled && isCurrent();
  const movement = useAnimatedScrollHandler(
    (event) => {
      if (uiBinding.get() === token && Number.isFinite(event.contentOffset.y))
        distance.set(geometry.height > 0 ? event.contentOffset.y - geometry.height : 0);
    },
    [geometry.height, token, uiBinding],
  );
  const background = useAnimatedStyle(() => ({
    opacity: Math.max(0, 1 - Math.abs(distance.get()) / 80),
  }));
  const controls = useAnimatedStyle(() => ({
    opacity: reveal.get() * Math.max(0, 1 - Math.abs(distance.get()) / 50),
  }));
  const finish = ({ nativeEvent }: NativeSyntheticEvent<NativeScrollEvent>) => {
    if (!admitted() || dragging.current !== binding || dismissing.current) return;
    dragging.current = null;
    const offset = nativeEvent.contentOffset.y - geometry.height;
    const velocity = nativeEvent.velocity?.y ?? 0;
    if (!Number.isFinite(offset) || !Number.isFinite(velocity)) return;
    // Telegram's vertical gallery has a3×viewport content area: height/12 == viewport/4.
    if (Math.abs(offset) > geometry.height / 4 || Math.abs(velocity) > 1) {
      dismissing.current = true;
      onClose();
    } else scroll.current?.scrollTo({ y: geometry.height, animated: !reducedMotion });
  };
  return (
    <View
      testID="attachment-gallery-surface"
      style={{ flex: 1 }}
      onAccessibilityEscape={() => {
        if (committed.current === binding && isCurrent()) onClose();
      }}
      onLayout={({ nativeEvent: { layout } }) => {
        if (mountedOwner.current !== owner) return;
        if (
          !Number.isFinite(layout.width) ||
          !Number.isFinite(layout.height) ||
          layout.width < 1 ||
          layout.height < 1 ||
          (layout.width === liveGeometry.current.width &&
            layout.height === liveGeometry.current.height)
        )
          return;
        committed.current = null;
        uiBinding.set('');
        const next = {
          width: layout.width,
          height: layout.height,
          epoch: liveGeometry.current.epoch + 1,
        };
        liveGeometry.current = next;
        setGeometry(next);
      }}
    >
      <Animated.View
        pointerEvents="none"
        style={[{ position: 'absolute', inset: 0, backgroundColor: '#000000' }, background]}
      />
      <Animated.ScrollView
        ref={scroll}
        testID="attachment-gallery-dismiss"
        style={{ flex: 1 }}
        contentContainerStyle={{ height: geometry.height * 3 }}
        contentOffset={{ x: 0, y: geometry.height }}
        scrollEnabled={!disabled && geometry.height > 0}
        directionalLockEnabled
        bounces={false}
        decelerationRate={0}
        showsVerticalScrollIndicator={false}
        automaticallyAdjustContentInsets={false}
        contentInsetAdjustmentBehavior="never"
        scrollEventThrottle={16}
        onScroll={movement}
        onScrollBeginDrag={() => {
          if (admitted()) dragging.current = binding;
        }}
        onScrollEndDrag={finish}
      >
        <View style={{ height: geometry.height }} />
        <View
          style={{
            width: geometry.width || '100%',
            height: geometry.height || undefined,
            flex: geometry.height ? undefined : 1,
          }}
        >
          {children}
        </View>
        <View style={{ height: geometry.height }} />
      </Animated.ScrollView>
      <Animated.View
        pointerEvents={controlsHidden ? 'none' : 'box-none'}
        accessibilityElementsHidden={controlsHidden}
        importantForAccessibility={controlsHidden ? 'no-hide-descendants' : 'auto'}
        style={[{ position: 'absolute', inset: 0 }, controls]}
      >
        {overlay}
      </Animated.View>
    </View>
  );
}
