import { Image } from 'expo-image';
import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { ScrollView, View, type ScrollViewInstance } from 'react-native';
import {
  InterceptingGestureDetector,
  VirtualGestureDetector,
  useExclusiveGestures,
  useTapGesture,
} from 'react-native-gesture-handler';

/** Native UIScrollView owns pinch/pan. Every callback remains bound to this displayed asset. */
export function AttachmentImageSurface({
  identity,
  uri,
  label,
  active,
  disabled,
  reducedMotion,
  isCurrent,
  onZoomChange,
  onToggleControls,
}: {
  identity: string;
  uri: string;
  label: string;
  active: boolean;
  disabled: boolean;
  reducedMotion: boolean;
  isCurrent(): boolean;
  onZoomChange?(identity: string, zoomed: boolean): void;
  onToggleControls?(): void;
}) {
  const view = useRef<ScrollViewInstance | null>(null);
  const [geometry, setGeometry] = useState({ width: 0, height: 0 });
  const [ratio, setRatio] = useState(1);
  const [zoomed, setZoomed] = useState(false);
  const zoom = useRef(1);
  const binding = useMemo(
    () => ({ identity, uri, geometry, ratio, active, disabled }),
    [identity, uri, geometry, ratio, active, disabled],
  );
  const committed = useRef<typeof binding | null>(null);
  const maximum =
    geometry.width > 0 && geometry.height > 0
      ? Math.max(
          3,
          geometry.width / Math.min(geometry.width, geometry.height * ratio),
          geometry.height / Math.min(geometry.height, geometry.width / ratio),
        )
      : 3;
  const admitted = () => committed.current === binding && active && !disabled && isCurrent();
  const reset = () => {
    zoom.current = 1;
    setZoomed(false);
    onZoomChange?.(identity, false);
    if (geometry.width > 0 && geometry.height > 0)
      // RN0.88's native command still reads its second animation argument.
      // Supply both to preserve Reduce Motion without triggering its deprecation warning.
      view.current?.scrollResponderZoomTo({ x: 0, y: 0, ...geometry, animated: false }, false);
  };
  useLayoutEffect(() => {
    committed.current = binding;
    if (!active || disabled) reset();
    return () => {
      if (committed.current === binding) committed.current = null;
    };
    // Native zoom is local presentation; current callbacks/selection remain outside it.
    // oxlint-disable-next-line react/exhaustive-deps
  }, [binding]);
  useLayoutEffect(() => {
    reset();
    // A new positive orientation frame resets its native zoom rather than reusing old coordinates.
    // oxlint-disable-next-line react/exhaustive-deps
  }, [geometry, uri]);
  const doubleTap = useTapGesture({
    enabled: active && !disabled,
    numberOfTaps: 2,
    runOnJS: true,
    onDeactivate: (event) => {
      if (event.canceled || !admitted() || !geometry.width || !geometry.height) return;
      const scale = zoom.current > 1.001 ? 1 : maximum;
      const width = geometry.width / scale,
        height = geometry.height / scale;
      const target =
        scale === 1
          ? { x: 0, y: 0, width, height }
          : { x: event.x - width / 2, y: event.y - height / 2, width, height };
      zoom.current = scale;
      setZoomed(scale > 1.001);
      onZoomChange?.(identity, scale > 1.001);
      view.current?.scrollResponderZoomTo({ ...target, animated: !reducedMotion }, !reducedMotion);
    },
  });
  const tap = useTapGesture({
    enabled: active && !disabled && Boolean(onToggleControls),
    runOnJS: true,
    onDeactivate: (event) => {
      if (!event.canceled && admitted()) onToggleControls?.();
    },
  });
  const gestures = useExclusiveGestures(doubleTap, tap);
  return (
    <InterceptingGestureDetector>
      <VirtualGestureDetector gesture={gestures}>
        <View
          collapsable={false}
          testID={`attachment-image-surface-${identity}`}
          style={{ flex: 1 }}
          onLayout={({ nativeEvent: { layout } }) => {
            if (committed.current !== binding || !isCurrent()) return;
            if (
              !Number.isFinite(layout.width) ||
              !Number.isFinite(layout.height) ||
              layout.width <= 0 ||
              layout.height <= 0
            )
              return;
            if (layout.width !== geometry.width || layout.height !== geometry.height) {
              committed.current = null;
              setGeometry({ width: layout.width, height: layout.height });
            }
          }}
        >
          <ScrollView
            ref={view}
            testID={`attachment-image-zoom-${identity}`}
            style={{ flex: 1 }}
            contentContainerStyle={{
              width: geometry.width || undefined,
              height: geometry.height || undefined,
            }}
            scrollEnabled={active && !disabled && zoomed}
            minimumZoomScale={1}
            maximumZoomScale={maximum}
            pinchGestureEnabled={active && !disabled}
            bouncesZoom
            centerContent
            bounces={false}
            showsHorizontalScrollIndicator={false}
            showsVerticalScrollIndicator={false}
            automaticallyAdjustContentInsets={false}
            contentInsetAdjustmentBehavior="never"
            scrollEventThrottle={16}
            onScroll={({ nativeEvent }) => {
              const scale = nativeEvent.zoomScale;
              if (!admitted() || typeof scale !== 'number' || !Number.isFinite(scale) || scale <= 0)
                return;
              zoom.current = scale;
              const next = Math.abs(scale - 1) > 0.001;
              setZoomed(next);
              onZoomChange?.(identity, next);
            }}
          >
            <Image
              testID={`attachment-approval-image-${identity}`}
              source={{ uri }}
              recyclingKey={`${identity}:${uri}`}
              cachePolicy="none"
              contentFit="contain"
              accessibilityLabel={label}
              onLoad={({ source }) => {
                if (committed.current !== binding || source.url !== uri || !isCurrent()) return;
                if (
                  Number.isFinite(source.width) &&
                  Number.isFinite(source.height) &&
                  source.width > 0 &&
                  source.height > 0
                )
                  setRatio(source.width / source.height);
              }}
              style={{ width: geometry.width || '100%', height: geometry.height || '100%' }}
            />
          </ScrollView>
        </View>
      </VirtualGestureDetector>
    </InterceptingGestureDetector>
  );
}
