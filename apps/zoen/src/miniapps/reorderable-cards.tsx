import { useFocusEffect } from 'expo-router';
import * as Haptics from 'expo-haptics';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { AccessibilityInfo, AppState, View } from 'react-native';
import { GestureDetector, usePanGesture } from 'react-native-gesture-handler';
import Animated, {
  cancelAnimation,
  Easing,
  measure,
  scrollTo,
  useAnimatedScrollHandler,
  useAnimatedStyle,
  useFrameCallback,
  useSharedValue,
  withRepeat,
  withSequence,
  withSpring,
  withTiming,
  type SharedValue,
  type AnimatedRef,
} from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';
import { useTheme } from '../theme';
import { CompactMiniappCard } from './compact-card';
import { APP_CARD_METRICS } from './app-card-layout';
import { t } from '../i18n';
import type { ItemDetail } from '../core/generated/ItemDetail';
import {
  CardReorderSession,
  cardDropIndex,
  cardEdgeVelocity,
  moveCard,
  sameCardOrder,
} from './card-reorder';

export function useCardScrollPort(horizontal: boolean) {
  const offset = useSharedValue(0);
  const extent = useSharedValue(0);
  const minimum = useSharedValue(0);
  const onScroll = useAnimatedScrollHandler((event) => {
    offset.set(horizontal ? event.contentOffset.x : event.contentOffset.y);
    minimum.set(horizontal ? -event.contentInset.left : -event.contentInset.top);
  });
  return { offset, extent, minimum, horizontal, onScroll };
}
type ScrollPort = ReturnType<typeof useCardScrollPort>;
type GestureAdmission = { generation: number; activation: number };
type Props = {
  items: readonly ItemDetail[];
  /** Cancels a gesture when a larger ordered scope changes (Home's available subset). */
  contextKey?: string;
  columns: number;
  horizontal?: boolean;
  scrollPort: ScrollPort;
  scrollRef: AnimatedRef<Animated.ScrollView>;
  editing: boolean;
  onEditingChange: (editing: boolean) => void;
  onPendingChange?: (pending: boolean) => void;
  onReorder: (ids: readonly string[], expected: readonly string[]) => Promise<unknown>;
  onOpen: (item: ItemDetail) => void;
  renderAccessory?: (item: ItemDetail) => ReactNode;
};
function feedback(lift = false) {
  if (process.env.EXPO_OS !== 'ios') return;
  void (
    lift ? Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light) : Haptics.selectionAsync()
  ).catch(() => {});
}
/** Transient gesture geometry; the existing preference controller is the only order writer. */
export function ReorderableAppCards({
  items,
  contextKey = '',
  columns,
  horizontal = false,
  scrollPort,
  scrollRef,
  editing,
  onEditingChange,
  onPendingChange,
  onReorder,
  onOpen,
  renderAccessory,
}: Props) {
  const { reducedMotion, colors } = useTheme();
  const { size, gap } = APP_CARD_METRICS;
  const { offset: scrollOffset, extent: scrollExtent, minimum: minimumOffset } = scrollPort;
  const ids = items.map((item) => item.id);
  const signature = JSON.stringify(ids);
  const lifetime = useRef({
    mounted: true,
    generation: 0,
    focused: false,
    foreground: AppState.currentState === 'active',
  });
  const latest = useRef({ onReorder, onEditingChange, onOpen, items });
  useLayoutEffect(() => {
    latest.current = { onReorder, onEditingChange, onOpen, items };
  });
  const [session] = useState(() => new CardReorderSession(ids, onReorder));
  useLayoutEffect(() => session.setPersist(onReorder), [session, onReorder]);
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const order = useSharedValue<string[]>(ids);
  const active = useSharedValue<string | null>(null);
  const consumed = useSharedValue(false);
  const nativeEditing = useSharedValue(false);
  const nativeGeneration = useSharedValue(0);
  const activation = useSharedValue(0);
  const admission = useSharedValue<GestureAdmission | null>(null);
  const dragX = useSharedValue(0);
  const dragY = useSharedValue(0);
  const pointer = useSharedValue(0);
  const dragScroll = useSharedValue(0);
  const startOffset = useSharedValue(0);
  const baseline = useSharedValue<string[]>(ids);
  const height = size;
  const strideX = size + gap;
  const strideY = height + gap;
  const count = items.length;
  const geometry = `${size}:${height}:${columns}:${horizontal}`;
  const cancel = useCallback(() => {
    // The JS fence changes before any queued native-to-RN callback can be admitted.
    // UI publication may lag; an activation capturing an older epoch is safely rejected.
    lifetime.current.generation++;
    nativeGeneration.set(lifetime.current.generation);
    admission.set(null);
    nativeEditing.set(false);
    active.set(null);
    consumed.set(true);
    order.set([...session.getSnapshot().order]);
  }, [active, consumed, order, session, nativeGeneration, nativeEditing, admission]);
  useLayoutEffect(() => {
    session.update(JSON.parse(signature) as string[]);
    cancel();
  }, [signature, geometry, contextKey, session, cancel]);
  useLayoutEffect(() => {
    if (editing) session.edit();
    else {
      cancel();
      session.retire();
    }
  }, [editing, session, cancel]);
  useEffect(() => {
    if (!active.get()) order.set([...state.order]);
    onPendingChange?.(state.pending);
  }, [state.order, state.pending, onPendingChange, active, order]);
  const retire = useCallback(() => {
    cancel();
    session.retire();
    latest.current.onEditingChange(false);
  }, [cancel, session]);
  useFocusEffect(
    useCallback(() => {
      lifetime.current.focused = true;
      return () => {
        lifetime.current.focused = false;
        retire();
      };
    }, [retire]),
  );
  useEffect(() => {
    const scope = lifetime.current;
    scope.mounted = true;
    const subscription = AppState.addEventListener('change', (value) => {
      lifetime.current.foreground = value === 'active';
      if (value !== 'active') retire();
    });
    return () => {
      scope.mounted = false;
      subscription.remove();
      // Do not set parent state while this scope is unmounting.
      cancel();
      session.retire();
    };
  }, [retire, cancel, session]);
  function canInteract() {
    const value = lifetime.current;
    return value.mounted && value.focused && value.foreground;
  }
  function admitted(token: GestureAdmission) {
    return (
      token.generation === lifetime.current.generation && token.activation === activation.get()
    );
  }
  function moveFeedback(token: GestureAdmission) {
    if (canInteract() && admitted(token)) feedback();
  }
  function begin(token?: GestureAdmission) {
    if (!canInteract() || (token && !admitted(token)) || session.getSnapshot().pending) return;
    const entered = !session.getSnapshot().editing;
    session.edit();
    latest.current.onEditingChange(true);
    feedback(true);
    if (entered) AccessibilityInfo.announceForAccessibility(t('home.rearranging'));
  }
  async function drop(
    next: readonly string[],
    expected: readonly string[],
    id: string,
    token?: GestureAdmission,
  ) {
    if (!canInteract() || (token && !admitted(token))) return;
    const accepted = await session.drop(next, expected);
    if (canInteract() && (!token || admitted(token)) && !active.get())
      order.set([...session.getSnapshot().order]);
    if (accepted) {
      feedback();
      const index = next.indexOf(id);
      const item = latest.current.items.find((value) => value.id === id);
      if (item && canInteract())
        AccessibilityInfo.announceForAccessibility(
          t('home.movedPosition', { name: item.title, position: index + 1, count: next.length }),
        );
    }
  }
  useFrameCallback((frame) => {
    'worklet';
    if (!active.get()) return;
    const viewport = measure(scrollRef);
    if (!viewport) return;
    const length = horizontal ? viewport.width : viewport.height;
    const start = horizontal ? viewport.pageX : viewport.pageY;
    const velocity = cardEdgeVelocity(pointer.get(), start, length);
    if (!velocity) return;
    const before = scrollOffset.get();
    const next = Math.max(
      minimumOffset.get(),
      Math.min(
        Math.max(minimumOffset.get(), scrollExtent.get() - length),
        before + velocity * (Math.min(frame.timeSincePreviousFrame ?? 16, 32) / 1000),
      ),
    );
    if (next === before) return;
    scrollOffset.set(next);
    scrollTo(scrollRef, horizontal ? next : 0, horizontal ? 0 : next, false);
    dragScroll.set(next - startOffset.get());
    const x = dragX.get() + (horizontal ? dragScroll.get() : 0);
    const y = dragY.get() + (horizontal ? 0 : dragScroll.get());
    const target = cardDropIndex(x, y, count, horizontal ? count : columns, strideX, strideY);
    const previous = order.get();
    const moved = moveCard(previous, active.get()!, target);
    if (!sameCardOrder(previous, moved)) {
      order.set(moved);
      const token = admission.get();
      if (token) scheduleOnRN(moveFeedback, token);
    }
  });
  const nativeReorder = process.env.EXPO_OS === 'ios';
  const enabled = nativeReorder && !state.pending;
  return (
    <View
      testID="reorderable-app-cards"
      style={{
        width: horizontal ? count * strideX - gap : columns * size + (columns - 1) * gap,
        alignSelf: 'center',
        height: horizontal ? height : Math.ceil(count / columns) * strideY - gap,
      }}
    >
      {items.map((item, index) => (
        <DraggableCard
          key={item.id}
          id={item.id}
          index={index}
          count={count}
          columns={horizontal ? count : columns}
          size={size}
          strideX={strideX}
          strideY={strideY}
          horizontal={horizontal}
          enabled={enabled}
          editing={state.editing}
          reducedMotion={reducedMotion}
          outlineColor={colors.text}
          order={order}
          active={active}
          consumed={consumed}
          nativeEditing={nativeEditing}
          nativeGeneration={nativeGeneration}
          activation={activation}
          admission={admission}
          dragX={dragX}
          dragY={dragY}
          pointer={pointer}
          dragScroll={dragScroll}
          startOffset={startOffset}
          baseline={baseline}
          scrollPort={scrollPort}
          onBegin={begin}
          onMoveFeedback={moveFeedback}
          onDrop={drop}
        >
          <CompactMiniappCard
            item={item}
            accessory={renderAccessory?.(item)}
            editing={state.editing}
            onPress={() => {
              if (
                canInteract() &&
                !session.getSnapshot().editing &&
                !session.getSnapshot().pending &&
                (!nativeReorder || (!consumed.get() && !nativeEditing.get()))
              )
                latest.current.onOpen(item);
            }}
            onAccessibilityTap={() => {
              if (
                canInteract() &&
                !session.getSnapshot().editing &&
                !session.getSnapshot().pending &&
                (!nativeReorder || !nativeEditing.get())
              )
                latest.current.onOpen(item);
            }}
            accessibilityHint={
              nativeReorder
                ? state.editing
                  ? t('home.rearrangeHint')
                  : t('home.holdToRearrange')
                : t('miniapps.openMiniapp')
            }
            accessibilityActions={
              !nativeReorder
                ? undefined
                : state.editing
                  ? [
                      ...(state.order.indexOf(item.id) > 0
                        ? [{ name: 'moveEarlier', label: t('home.moveEarlier') }]
                        : []),
                      ...(state.order.indexOf(item.id) < count - 1
                        ? [{ name: 'moveLater', label: t('home.moveLater') }]
                        : []),
                    ]
                  : [{ name: 'rearrange', label: t('home.rearrange') }]
            }
            onAccessibilityAction={(event) => {
              const action = event.nativeEvent.actionName;
              if (action === 'rearrange') begin();
              else if (action === 'moveEarlier' || action === 'moveLater') {
                const current = session.getSnapshot();
                const position = current.order.indexOf(item.id);
                void drop(
                  moveCard(current.order, item.id, position + (action === 'moveEarlier' ? -1 : 1)),
                  current.order,
                  item.id,
                );
              }
            }}
          />
        </DraggableCard>
      ))}
    </View>
  );
}

type DragValues = {
  order: SharedValue<string[]>;
  active: SharedValue<string | null>;
  consumed: SharedValue<boolean>;
  nativeEditing: SharedValue<boolean>;
  nativeGeneration: SharedValue<number>;
  activation: SharedValue<number>;
  admission: SharedValue<GestureAdmission | null>;
  dragX: SharedValue<number>;
  dragY: SharedValue<number>;
  pointer: SharedValue<number>;
  dragScroll: SharedValue<number>;
  startOffset: SharedValue<number>;
  baseline: SharedValue<string[]>;
};
function DraggableCard({
  id,
  index,
  count,
  columns,
  size,
  strideX,
  strideY,
  horizontal,
  enabled,
  editing,
  reducedMotion,
  outlineColor,
  order,
  active,
  consumed,
  nativeEditing,
  nativeGeneration,
  activation,
  admission,
  dragX,
  dragY,
  pointer,
  dragScroll,
  startOffset,
  baseline,
  scrollPort,
  onBegin,
  onMoveFeedback,
  onDrop,
  children,
}: DragValues & {
  id: string;
  index: number;
  count: number;
  columns: number;
  size: number;
  strideX: number;
  strideY: number;
  horizontal: boolean;
  enabled: boolean;
  editing: boolean;
  reducedMotion: boolean;
  outlineColor: string;
  scrollPort: ScrollPort;
  onBegin: (token: GestureAdmission) => void;
  onMoveFeedback: (token: GestureAdmission) => void;
  onDrop: (
    next: readonly string[],
    expected: readonly string[],
    id: string,
    token: GestureAdmission,
  ) => void;
  children: ReactNode;
}) {
  const scrollOffset = scrollPort.offset;
  const wobble = useSharedValue(0);
  const originX = useSharedValue(0);
  const originY = useSharedValue(0);
  useEffect(() => {
    cancelAnimation(wobble);
    if (editing && !reducedMotion) {
      const direction = index % 2 ? 1 : -1;
      wobble.set(
        withRepeat(
          withSequence(
            withTiming(direction * 0.7, { duration: 130, easing: Easing.inOut(Easing.sin) }),
            withTiming(direction * -0.7, { duration: 150, easing: Easing.inOut(Easing.sin) }),
          ),
          -1,
          true,
        ),
      );
    } else wobble.set(0);
    return () => cancelAnimation(wobble);
  }, [editing, reducedMotion, index, wobble]);
  const pan = usePanGesture({
    enabled,
    activateAfterLongPress: editing ? 120 : 450,
    minDistance: 0,
    maxPointers: 1,
    cancelsTouchesInView: true,
    onTouchesDown: () => {
      'worklet';
      consumed.set(false);
    },
    onActivate: (event) => {
      'worklet';
      if (active.get()) return;
      consumed.set(true);
      nativeEditing.set(true);
      const token = { generation: nativeGeneration.get(), activation: activation.get() + 1 };
      activation.set(token.activation);
      admission.set(token);
      active.set(id);
      baseline.set([...order.get()]);
      const position = order.get().indexOf(id);
      originX.set((position % columns) * strideX);
      originY.set(Math.floor(position / columns) * strideY);
      dragX.set(originX.get());
      dragY.set(originY.get());
      pointer.set(horizontal ? event.absoluteX : event.absoluteY);
      startOffset.set(scrollOffset.get());
      dragScroll.set(0);
      scheduleOnRN(onBegin, token);
    },
    onUpdate: (event) => {
      'worklet';
      if (active.get() !== id) return;
      dragX.set(originX.get() + event.translationX);
      dragY.set(originY.get() + event.translationY);
      pointer.set(horizontal ? event.absoluteX : event.absoluteY);
      dragScroll.set(scrollOffset.get() - startOffset.get());
      const target = cardDropIndex(
        dragX.get() + (horizontal ? dragScroll.get() : 0),
        dragY.get() + (horizontal ? 0 : dragScroll.get()),
        count,
        columns,
        strideX,
        strideY,
      );
      const previous = order.get();
      const moved = moveCard(previous, id, target);
      if (!sameCardOrder(previous, moved)) {
        order.set(moved);
        const token = admission.get();
        if (token) scheduleOnRN(onMoveFeedback, token);
      }
    },
    onDeactivate: (event) => {
      'worklet';
      if (active.get() !== id) return;
      const token = admission.get();
      if (event.canceled) order.set([...baseline.get()]);
      else if (token) scheduleOnRN(onDrop, [...order.get()], [...baseline.get()], id, token);
      active.set(null);
    },
    onFinalize: () => {
      'worklet';
      if (active.get() === id) {
        order.set([...baseline.get()]);
        active.set(null);
      }
    },
  });
  const style = useAnimatedStyle(() => {
    const dragging = active.get() === id;
    const position = Math.max(0, order.get().indexOf(id));
    const spring = { damping: 22, stiffness: 240 };
    const x = dragging
      ? dragX.get() + (horizontal ? dragScroll.get() : 0)
      : (position % columns) * strideX;
    const y = dragging
      ? dragY.get() + (horizontal ? 0 : dragScroll.get())
      : Math.floor(position / columns) * strideY;
    return {
      zIndex: dragging ? 10 : 0,
      shadowOpacity: dragging ? 0.18 : 0,
      transform: [
        { translateX: dragging || reducedMotion ? x : withSpring(x, spring) },
        { translateY: dragging || reducedMotion ? y : withSpring(y, spring) },
        { scale: reducedMotion ? 1 : withSpring(dragging ? 1.035 : 1, spring) },
        { rotateZ: `${dragging || reducedMotion ? 0 : wobble.get()}deg` },
      ],
    };
  });
  return (
    <GestureDetector gesture={pan}>
      <Animated.View
        collapsable={false}
        style={[
          {
            position: 'absolute',
            width: size,
            height: size,
            top: 0,
            left: 0,
            shadowColor: '#000000',
            shadowRadius: 16,
            shadowOffset: { width: 0, height: 8 },
          },
          style,
        ]}
      >
        {editing ? (
          <View
            pointerEvents="none"
            accessible={false}
            style={{
              position: 'absolute',
              inset: -2,
              borderRadius: APP_CARD_METRICS.radius + 2,
              borderCurve: 'continuous',
              borderWidth: 1,
              borderColor: outlineColor,
              opacity: 0.22,
            }}
          />
        ) : null}
        {children}
      </Animated.View>
    </GestureDetector>
  );
}
