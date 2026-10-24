import { useFocusEffect } from 'expo-router';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { AppState } from 'react-native';
import type { FlashListRef } from '@shopify/flash-list';
import type { SharedValue } from 'react-native-reanimated';
import type { ConversationTimelineRow } from '../core/conversation-timeline';
import type { ConversationReveal } from '../core/conversation-reveal';
import { ConversationRestoreObservation } from '../core/conversation-restore-observation';
import { navigationMetricsAvailable, type NavigationMetricsHandle } from './navigation-container';

const qa = process.env.EXPO_PUBLIC_ZOEN_REVEAL_QA === '1';
type Options = {
  reveal: ConversationReveal;
  focused: boolean;
  searchActive: boolean;
  current(): boolean;
  list: { current: FlashListRef<ConversationTimelineRow> | null };
  rows: number;
  keyboard: { progress: SharedValue<number>; height: SharedValue<number> };
  reply: boolean;
  editing: boolean;
};

/** The same bound UIKit observer used by search, extended only in QA builds to
 * observe the closed→search→restored path. It never changes scrolling or insets. */
export function useChatNativeMetrics(options: Options) {
  const { reveal, focused, searchActive } = options;
  const ref = useRef<NavigationMetricsHandle | null>(null);
  const latest = useRef(options);
  useLayoutEffect(() => {
    latest.current = options;
  }, [options]);
  const [activation, setActivation] = useState(0);
  const observationRef = useRef<ConversationRestoreObservation | null>(null);
  useFocusEffect(
    useCallback(() => {
      if (!qa) return;
      setActivation((n) => n + 1);
      return () => observationRef.current?.retire();
    }, []),
  );
  useEffect(() => {
    if (!qa) return;
    const subscription = AppState.addEventListener('change', () => {
      observationRef.current?.retire();
      setActivation((n) => n + 1);
    });
    return () => subscription.remove();
  }, []);
  const observation = useMemo(() => {
    if (!qa || !focused || AppState.currentState !== 'active') return null;
    return new ConversationRestoreObservation(reveal.nativeBinding, activation);
  }, [reveal, focused, activation]);
  useLayoutEffect(() => {
    observation?.connect({
      current: () => AppState.currentState === 'active' && latest.current.current(),
      context: () => {
        const o = latest.current;
        let list = null;
        try {
          const native = o.list.current;
          if (native) {
            const tail = o.rows ? native.getLayout(o.rows - 1) : undefined;
            list = {
              firstItemOffset: native.getFirstItemOffset(),
              childHeight: native.getChildContainerDimensions().height,
              windowHeight: native.getWindowSize().height,
              lastRowY: tail?.y ?? null,
              lastRowHeight: tail?.height ?? null,
            };
          }
        } catch {
          /* SDK layout is not yet available; native geometry is still recorded. */
        }
        return {
          searchActive: o.searchActive,
          reply: o.reply,
          editing: o.editing,
          keyboardHeight: o.keyboard.height.value,
          keyboardProgress: o.keyboard.progress.value,
          list,
        };
      },
      emit: (record) => console.info(`[ZoenRestore] ${JSON.stringify(record)}`),
    });
  }, [observation]);
  useLayoutEffect(() => {
    observationRef.current = observation;
    return () => observation?.retire();
  }, [observation]);
  const read = useCallback(async () => {
    const native = ref.current;
    if (!native) return null;
    const packet = await native.sampleScroll(reveal.nativeBinding);
    return ref.current === native ? packet : null;
  }, [reveal]);
  useLayoutEffect(() => {
    void observation?.sample(read);
  }, [observation, read, searchActive, options.reply, options.editing]);
  const navigationMetrics = useMemo(
    () => ({
      binding: focused && (searchActive || observation) ? reveal.nativeBinding : '',
      ref,
      changed: (packet: Parameters<ConversationReveal['nativeMetricsChanged']>[0]) => {
        if (!latest.current.current()) return;
        if (latest.current.searchActive) reveal.nativeMetricsChanged(packet);
        observation?.receive(packet);
      },
    }),
    [focused, searchActive, observation, reveal],
  );
  return { navigationMetrics, observe: navigationMetricsAvailable ? read : undefined };
}
