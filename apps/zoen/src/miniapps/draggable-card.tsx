import { useEffect, type ReactNode } from 'react';
import { View } from 'react-native';
import { GestureDetector, usePanGesture } from 'react-native-gesture-handler';
import Animated, {
  cancelAnimation,
  Easing,
  useAnimatedStyle,
  useSharedValue,
  withRepeat,
  withSequence,
  withSpring,
  withTiming,
} from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';
import { APP_CARD_METRICS } from './app-card-layout';
import { cardDropIndex, moveCard, sameCardOrder } from './card-reorder';
import type { DragValues, GestureAdmission, ScrollPort } from './card-scroll-port';

export function DraggableCard({
  id,
  index,
  count,
  columns,
  size,
  strideX,
  strideY,
  horizontal,
  enabled,
  controlsVisible,
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
  controlsVisible: boolean;
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
    // Negative native hitSlop removes the entire controls row before recognition,
    // so holding X/Pin cannot activate a pan that cancels the button touch.
    hitSlop: controlsVisible
      ? { top: -(APP_CARD_METRICS.padding + APP_CARD_METRICS.menuSize) }
      : undefined,
    // On iOS, zero still schedules a long press that fails after10pt of movement.
    // Omit it while editing so the pan does not require a second stationary hold.
    activateAfterLongPress: editing ? undefined : 450,
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
        { translateX: !editing || dragging || reducedMotion ? x : withSpring(x, spring) },
        { translateY: !editing || dragging || reducedMotion ? y : withSpring(y, spring) },
        { scale: !editing || reducedMotion ? 1 : withSpring(dragging ? 1.035 : 1, spring) },
        { rotateZ: `${dragging || reducedMotion ? 0 : wobble.get()}deg` },
      ],
    };
  });
  return (
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
      <GestureDetector gesture={pan}>
        <View collapsable={false} style={{ width: size, height: size }}>
          {children}
          {editing ? (
            <View
              pointerEvents="none"
              accessible={false}
              style={{
                position: 'absolute',
                inset: 0,
                borderRadius: APP_CARD_METRICS.radius,
                borderCurve: 'continuous',
                borderWidth: 1,
                borderColor: outlineColor,
                opacity: 0.22,
              }}
            />
          ) : null}
        </View>
      </GestureDetector>
    </Animated.View>
  );
}
