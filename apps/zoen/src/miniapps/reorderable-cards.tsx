import * as Haptics from 'expo-haptics';
import { useFocusEffect } from 'expo-router';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { AccessibilityInfo, AppState, View } from 'react-native';
import type Animated from 'react-native-reanimated';
import {
  measure,
  scrollTo,
  useFrameCallback,
  useSharedValue,
  type AnimatedRef,
} from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { APP_CARD_METRICS } from './app-card-layout';
import { AppCardEditActions } from './card-edit-actions';
import {
  cardDropIndex,
  cardEdgeVelocity,
  CardReorderSession,
  moveCard,
  sameCardOrder,
} from './card-reorder';
import type { GestureAdmission, ScrollPort } from './card-scroll-port';
import { CompactMiniappCard } from './compact-card';
import { DraggableCard } from './draggable-card';
export { useCardScrollPort } from './card-scroll-port';

type Props = {
  items: readonly ItemDetail[];
  /** Cancels a gesture when a larger ordered scope changes (Home's available subset). */
  contextKey?: string;
  columns: number;
  horizontal?: boolean;
  scrollPort: ScrollPort;
  scrollRef: AnimatedRef<Animated.ScrollView>;
  editing: boolean;
  onEditingChange: (editing: boolean) => boolean | void;
  onPendingChange?: (pending: boolean) => void;
  onReorder: (ids: readonly string[], expected: readonly string[]) => Promise<unknown>;
  onOpen: (item: ItemDetail) => void;
  onRemove?: (item: ItemDetail) => Promise<unknown>;
  onTogglePin?: (item: ItemDetail) => Promise<unknown>;
  isPinned?: (id: string) => boolean;
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
  onRemove,
  onTogglePin,
  isPinned,
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
  const [actionPending, setActionPending] = useState(false);
  const actionBusy = useRef(false);
  const latest = useRef({
    onReorder,
    onEditingChange,
    onOpen,
    onRemove,
    onTogglePin,
    items,
  });
  useLayoutEffect(() => {
    latest.current = {
      onReorder,
      onEditingChange,
      onOpen,
      onRemove,
      onTogglePin,
      items,
    };
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
    onPendingChange?.(state.pending || actionPending);
  }, [state.order, state.pending, actionPending, onPendingChange, active, order]);
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
  function begin(id: string, token?: GestureAdmission) {
    if (
      !canInteract() ||
      (token && !admitted(token)) ||
      session.getSnapshot().pending ||
      actionBusy.current
    )
      return;
    if (!latest.current.items.some((item) => item.id === id)) return;
    const entered = !session.getSnapshot().editing;
    if (latest.current.onEditingChange(true) === false) {
      cancel();
      session.retire();
      return;
    }
    session.edit();
    feedback(true);
    if (entered) AccessibilityInfo.announceForAccessibility(t('home.rearranging'));
  }
  async function drop(
    next: readonly string[],
    expected: readonly string[],
    id: string,
    token?: GestureAdmission,
  ) {
    if (!canInteract() || actionBusy.current || (token && !admitted(token))) return;
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
  async function editAction(id: string, kind: 'remove' | 'pin', generation: number) {
    if (
      !canInteract() ||
      generation !== lifetime.current.generation ||
      !session.getSnapshot().editing ||
      session.getSnapshot().pending ||
      actionBusy.current ||
      active.get()
    )
      return;
    const item = latest.current.items.find((value) => value.id === id);
    const operation = kind === 'remove' ? latest.current.onRemove : latest.current.onTogglePin;
    if (!item || !operation) return;
    actionBusy.current = true;
    // Retire any native activation already queued before this presentation write.
    cancel();
    const operationGeneration = lifetime.current.generation;
    setActionPending(true);
    try {
      await operation(item);
      if (canInteract() && operationGeneration === lifetime.current.generation) feedback();
    } catch {
      // The scoped preference controller exposes the save error and retains committed layout.
    } finally {
      actionBusy.current = false;
      if (lifetime.current.mounted) setActionPending(false);
    }
  }
  function press(item: ItemDetail, accessibility = false) {
    if (
      !canInteract() ||
      actionBusy.current ||
      session.getSnapshot().pending ||
      !latest.current.items.some((value) => value.id === item.id)
    )
      return;
    if (
      !session.getSnapshot().editing &&
      (!nativeReorder || (!nativeEditing.get() && (accessibility || !consumed.get())))
    ) {
      latest.current.onOpen(item);
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
  const enabled = nativeReorder && !state.pending && !actionPending;
  const actionGeneration = lifetime.current.generation;
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
          controlsVisible={state.editing && !!(onRemove || onTogglePin)}
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
          onBegin={(token) => begin(item.id, token)}
          onMoveFeedback={moveFeedback}
          onDrop={drop}
        >
          <CompactMiniappCard
            item={item}
            editActions={
              state.editing && (onRemove || onTogglePin) ? (
                <AppCardEditActions
                  id={item.id}
                  name={item.title}
                  pinned={isPinned?.(item.id) ?? false}
                  disabled={state.pending || actionPending}
                  onRemove={
                    onRemove
                      ? () => void editAction(item.id, 'remove', actionGeneration)
                      : undefined
                  }
                  onTogglePin={
                    onTogglePin
                      ? () => void editAction(item.id, 'pin', actionGeneration)
                      : undefined
                  }
                />
              ) : undefined
            }
            editing={state.editing}
            onPress={() => press(item)}
            onAccessibilityTap={() => press(item, true)}
            onLongPress={!nativeReorder ? () => begin(item.id) : undefined}
            accessibilityHint={
              nativeReorder
                ? state.editing
                  ? t('home.rearrangeHint')
                  : t('home.holdToRearrange')
                : t('miniapps.openMiniapp')
            }
            accessibilityActions={
              state.editing
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
              if (action === 'rearrange') begin(item.id);
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
