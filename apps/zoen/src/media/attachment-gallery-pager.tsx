import { useLayoutEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import {
  ScrollView,
  View,
  type LayoutChangeEvent,
  type NativeScrollEvent,
  type NativeSyntheticEvent,
  type ScrollViewInstance,
} from 'react-native';

/** Presentation only. Selection and every durable mutation remain with the draft owner. */
export function AttachmentGalleryPager<Item extends { mutationId: string }>({
  items,
  selectedId,
  disabled,
  reducedMotion,
  isCurrent,
  onSelect,
  onDragStart,
  renderItem,
}: {
  items: readonly Item[];
  selectedId: string;
  disabled: boolean;
  reducedMotion: boolean;
  isCurrent(): boolean;
  onSelect(id: string): void;
  onDragStart(): void;
  renderItem(item: Item, active: boolean): ReactNode;
}) {
  const scroll = useRef<ScrollViewInstance | null>(null);
  const [geometry, setGeometry] = useState({ width: 0, height: 0, epoch: 0 });
  const liveGeometry = useRef(geometry);
  const ids = JSON.stringify(items.map((item) => item.mutationId));
  const binding = useMemo(
    () => ({ ids, geometry, disabled, selectedId }),
    [ids, geometry, disabled, selectedId],
  );
  const committed = useRef<typeof binding | null>(null);
  const dragging = useRef<typeof binding | null>(null);
  const lastGeometry = useRef<typeof geometry | null>(null);
  const index = Math.max(
    0,
    items.findIndex((item) => item.mutationId === selectedId),
  );
  useLayoutEffect(() => {
    committed.current = binding;
    dragging.current = null;
    return () => {
      if (committed.current === binding) committed.current = null;
    };
  }, [binding]);
  useLayoutEffect(() => {
    if (!geometry.width) return;
    scroll.current?.scrollTo({
      x: index * geometry.width,
      animated: !reducedMotion && lastGeometry.current === geometry,
    });
    lastGeometry.current = geometry;
  }, [geometry, index, ids, reducedMotion]);
  const admitted = () =>
    committed.current === binding && liveGeometry.current === geometry && !disabled && isCurrent();
  const measure = ({ nativeEvent: { layout } }: LayoutChangeEvent) => {
    if (
      !Number.isFinite(layout.width) ||
      !Number.isFinite(layout.height) ||
      layout.width < 1 ||
      layout.height < 1 ||
      (layout.width === liveGeometry.current.width && layout.height === liveGeometry.current.height)
    )
      return;
    // Fence queued UIKit callbacks immediately, including width A→B→A before React commits.
    committed.current = null;
    const next = {
      width: layout.width,
      height: layout.height,
      epoch: liveGeometry.current.epoch + 1,
    };
    liveGeometry.current = next;
    setGeometry(next);
  };
  const settle = ({ nativeEvent: { contentOffset } }: NativeSyntheticEvent<NativeScrollEvent>) => {
    if (
      !admitted() ||
      dragging.current !== binding ||
      !geometry.width ||
      !Number.isFinite(contentOffset.x)
    )
      return;
    const page = Math.round(contentOffset.x / geometry.width);
    if (page < 0 || page >= items.length) return;
    dragging.current = null;
    const item = items[page];
    if (item && item.mutationId !== selectedId) onSelect(item.mutationId);
  };
  return (
    <View testID="attachment-gallery-viewport" onLayout={measure} style={{ flex: 1 }}>
      <ScrollView
        ref={scroll}
        testID="attachment-gallery-pager"
        horizontal
        pagingEnabled
        directionalLockEnabled
        bounces={false}
        scrollEnabled={!disabled && items.length > 1 && geometry.width > 0}
        showsHorizontalScrollIndicator={false}
        keyboardDismissMode="on-drag"
        keyboardShouldPersistTaps="handled"
        onScrollBeginDrag={() => {
          if (admitted()) {
            dragging.current = binding;
            onDragStart();
          }
        }}
        onScrollEndDrag={(event) => {
          const { contentOffset, targetContentOffset, velocity } = event.nativeEvent;
          // UIKit does not emit momentum-end for a drag which stops exactly on a page.
          // A fractional/decelerating release must wait for its true momentum completion.
          if (
            velocity?.x === 0 &&
            targetContentOffset &&
            Math.abs(targetContentOffset.x - contentOffset.x) < 0.5 &&
            Math.abs(
              contentOffset.x - Math.round(contentOffset.x / geometry.width) * geometry.width,
            ) < 0.5
          )
            settle(event);
        }}
        onMomentumScrollEnd={settle}
        style={{ flex: 1 }}
      >
        {geometry.width > 0
          ? items.map((item, position) => (
              <View
                key={item.mutationId}
                testID={`attachment-gallery-page-${position}`}
                accessibilityElementsHidden={position !== index}
                importantForAccessibility={position === index ? 'auto' : 'no-hide-descendants'}
                style={{ width: geometry.width, height: geometry.height }}
              >
                {Math.abs(position - index) <= 1 ? renderItem(item, position === index) : null}
              </View>
            ))
          : null}
      </ScrollView>
    </View>
  );
}
