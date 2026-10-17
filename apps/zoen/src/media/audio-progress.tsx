import { useRef, useState } from 'react';
import { View } from 'react-native';
import {
  GestureDetector,
  useExclusiveGestures,
  usePanGesture,
  useTapGesture,
} from 'react-native-gesture-handler';
import { t } from '../i18n/index.ts';
import { useMessagePan, useMessagePressConsumption } from '../components/message-interaction';

const clamp = (value: number) => Math.max(0, Math.min(1, value));

/** Recorded peaks only; an absent waveform is a quiet baseline, never invented sound. */
export function AudioWaveform({
  levels,
  progress,
  ink,
  compact = false,
}: {
  levels: readonly number[];
  progress: number;
  ink: string;
  compact?: boolean;
}) {
  const count = compact ? 24 : 40;
  const peaks = Array.from({ length: count }, (_, bin) => {
    const start = Math.floor((bin * levels.length) / count);
    const end = Math.ceil(((bin + 1) * levels.length) / count);
    let peak = 0;
    for (let index = start; index < end; index++) {
      const level = levels[index];
      if (typeof level === 'number' && Number.isFinite(level)) peak = Math.max(peak, clamp(level));
    }
    return peak;
  });
  return (
    <View
      pointerEvents="none"
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={{ flex: 1, height: 32, flexDirection: 'row', gap: 2, alignItems: 'center' }}
    >
      {peaks.map((peak, index) => (
        <View
          key={index}
          style={{
            flex: 1,
            height: 3 + peak * 26,
            borderRadius: 2,
            backgroundColor: ink,
            opacity: (index + 0.5) / count <= progress ? 1 : 0.48,
          }}
        />
      ))}
    </View>
  );
}

/** Horizontal waveform control; vertical movement yields to the message timeline. */
export function AudioProgress({
  levels,
  progress,
  duration,
  ink,
  compact,
  disabled,
  onSeek,
}: {
  levels: readonly number[];
  progress: number;
  duration: number;
  ink: string;
  compact?: boolean;
  disabled: boolean;
  onSeek: (fraction: number) => void;
}) {
  const [width, setWidth] = useState(0);
  const [dragging, setDragging] = useState<number | null>(null);
  const drag = useRef<number | null>(null);
  const tapping = useRef<number | null>(null);
  const messagePan = useMessagePan();
  const consumePress = useMessagePressConsumption();
  const enabled = !disabled && duration > 0 && width > 0;
  const at = (x: number) => clamp(x / Math.max(1, width));
  const pan = usePanGesture({
    enabled,
    runOnJS: true,
    activeOffsetX: [-4, 4],
    failOffsetY: [-12, 12],
    block: messagePan,
    onBegin: () => {
      drag.current = width;
    },
    onActivate: consumePress,
    onUpdate: (event) => {
      if (enabled && drag.current === width) setDragging(at(event.x));
    },
    onDeactivate: (event) => {
      if (!event.canceled && enabled && drag.current === width) onSeek(at(event.x));
    },
    onFinalize: () => {
      drag.current = null;
      setDragging(null);
    },
  });
  const tap = useTapGesture({
    enabled,
    runOnJS: true,
    block: messagePan,
    onBegin: () => {
      tapping.current = width;
    },
    onActivate: consumePress,
    onDeactivate: (event) => {
      if (!event.canceled && enabled && tapping.current === width) onSeek(at(event.x));
    },
    onFinalize: () => {
      tapping.current = null;
    },
  });
  const gesture = useExclusiveGestures(pan, tap);
  const fraction = clamp(dragging ?? progress);
  const time = (seconds: number) =>
    `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, '0')}`;
  return (
    <GestureDetector gesture={gesture}>
      <View
        accessible
        collapsable={false}
        onTouchStart={consumePress}
        accessibilityRole="adjustable"
        accessibilityLabel={t('media.voiceMessage')}
        accessibilityState={{ disabled }}
        accessibilityValue={{
          min: 0,
          max: Math.round(duration),
          now: Math.round(fraction * duration),
          text: `${time(fraction * duration)} / ${time(duration)}`,
        }}
        accessibilityActions={[{ name: 'increment' }, { name: 'decrement' }]}
        onAccessibilityAction={(event) => {
          if (!enabled) return;
          const action = event.nativeEvent.actionName;
          if (action === 'increment' || action === 'decrement')
            onSeek(clamp(progress + (action === 'increment' ? 10 : -10) / duration));
        }}
        onLayout={(event) => setWidth(event.nativeEvent.layout.width)}
        style={{ flex: 1, minWidth: 36, height: 44, justifyContent: 'center' }}
      >
        <AudioWaveform levels={levels} progress={fraction} ink={ink} compact={compact} />
      </View>
    </GestureDetector>
  );
}
