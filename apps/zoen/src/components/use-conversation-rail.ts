import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { selectionAsync } from 'expo-haptics';
import { usePanGesture } from 'react-native-gesture-handler';
import {
  cancelAnimation,
  useDerivedValue,
  useSharedValue,
  withSpring,
} from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';
import {
  CONVERSATION_RAIL_PAN,
  railDragExpansion,
  railSnapExpansion,
  railVisibleHeight,
} from '../core/conversation-rail-motion';
import { useTheme } from '../theme';
import { useKeyboardMotion } from './keyboard-motion';

/** One presentation value drives the rail, its spacer and search obstruction. */
export function useConversationRail(scope: string, height: number, collapsible: boolean) {
  const { progress: keyboard } = useKeyboardMotion();
  const { reducedMotion, presentation } = useTheme();
  const owner = useMemo(() => ({ scope }), [scope]);
  const [intent, setIntent] = useState(() => ({ owner, collapsed: false, editing: false }));
  const collapsed = intent.owner === owner && intent.collapsed;
  const editing = intent.owner === owner && intent.editing;
  const expansion = useSharedValue(1);
  const target = useSharedValue(1);
  const origin = useSharedValue(1);
  const active = useSharedValue(false);
  const epoch = useSharedValue(0);
  const observedEpoch = useSharedValue(0);
  const committed = useRef<object | null>(null);
  useLayoutEffect(() => {
    committed.current = owner;
    epoch.set(epoch.get() + 1);
    cancelAnimation(expansion);
    expansion.set(1);
    target.set(1);
    active.set(false);
    return () => {
      committed.current = null;
      epoch.set(epoch.get() + 1);
      active.set(false);
      cancelAnimation(expansion);
    };
  }, [owner, epoch, expansion, target, active]);
  const visibleHeight = useDerivedValue(() =>
    railVisibleHeight(height, expansion.get(), keyboard.get(), collapsible),
  );
  const commit = useCallback(
    (next: number) => {
      if (committed.current !== owner || target.get() !== next) return;
      setIntent((previous) => ({
        owner,
        collapsed: next === 0,
        editing: previous.owner === owner && previous.editing,
      }));
      void selectionAsync().catch(() => {});
    },
    [owner, target],
  );
  const { mass, stiffness, damping } = presentation.motion.context;
  const settle = (next: number, velocity = 0) => {
    'worklet';
    const changed = target.get() !== next;
    target.set(next);
    expansion.set(
      reducedMotion
        ? next
        : withSpring(next, {
            mass,
            stiffness,
            damping,
            overshootClamping: true,
            velocity: height > 0 ? velocity / height : 0,
          }),
    );
    if (changed) scheduleOnRN(commit, next);
  };
  const gesture = usePanGesture({
    activeOffsetY: CONVERSATION_RAIL_PAN.activeOffsetY,
    failOffsetX: CONVERSATION_RAIL_PAN.failOffsetX,
    maxPointers: CONVERSATION_RAIL_PAN.maxPointers,
    enabled: collapsible && height > 0 && !editing,
    onActivate: () => {
      'worklet';
      if (keyboard.get() > 0) return;
      cancelAnimation(expansion);
      origin.set(expansion.get());
      observedEpoch.set(epoch.get());
      active.set(true);
    },
    onUpdate: (event) => {
      'worklet';
      if (!active.get() || observedEpoch.get() !== epoch.get()) return;
      if (keyboard.get() > 0) {
        active.set(false);
        settle(target.get());
        return;
      }
      expansion.set(railDragExpansion(origin.get(), event.translationY, height));
    },
    onDeactivate: (event) => {
      'worklet';
      if (!active.get() || observedEpoch.get() !== epoch.get()) return;
      active.set(false);
      settle(
        event.canceled || keyboard.get() > 0
          ? target.get()
          : railSnapExpansion(expansion.get(), event.velocityY, height),
        event.canceled ? 0 : event.velocityY,
      );
    },
    onFinalize: () => {
      'worklet';
      if (!active.get() || observedEpoch.get() !== epoch.get()) return;
      active.set(false);
      settle(target.get());
    },
  });
  const toggle = () => {
    if (committed.current !== owner || editing || !collapsible || height <= 0 || keyboard.get() > 0)
      return;
    settle(target.get() === 1 ? 0 : 1);
  };
  const onEditingChange = (next: boolean) => {
    if (committed.current !== owner) return;
    setIntent((previous) => ({
      owner,
      editing: next,
      collapsed: previous.owner === owner && previous.collapsed,
    }));
  };
  return {
    expansion,
    visibleHeight,
    gesture,
    collapsed,
    editing,
    onEditingChange,
    toggle,
  };
}

export type ConversationRailMotion = ReturnType<typeof useConversationRail>;
