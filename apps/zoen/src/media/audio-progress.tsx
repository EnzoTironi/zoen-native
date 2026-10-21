import { useLayoutEffect, useRef, useState } from 'react';
import { audioWaveformBars, telegramAudio } from '../core/telegram-audio-presentation';
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
  const [width, setWidth] = useState(0);
  const heights = audioWaveformBars(width, levels);
  const fraction = clamp(Number.isFinite(progress) ? progress : 0);
  return (
    <View
      pointerEvents="none"
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      onLayout={(event) => setWidth(event.nativeEvent.layout.width)}
      style={{
        flex: 1,
        height: compact ? telegramAudio.previewWaveformHeight : 32,
        flexDirection: 'row',
        gap: telegramAudio.waveformGap,
        alignItems: 'center',
        overflow: 'hidden',
      }}
    >
      {heights.map((height, index) => (
        <View
          key={index}
          style={{
            width: telegramAudio.waveformBar,
            height,
            borderRadius: telegramAudio.waveformBar / 2,
            backgroundColor: ink,
            opacity: (index + 0.5) / heights.length <= fraction ? 1 : 0.5,
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
  const admission = useRef<{ width: number; enabled: boolean; onSeek: typeof onSeek } | null>(null);
  useLayoutEffect(() => {
    admission.current = { width, enabled, onSeek };
    return () => {
      admission.current = null;
    };
  }, [width, enabled, onSeek]);
  const at = (x: number) => clamp(x / Math.max(1, width));
  const current = (captured: number | null) =>
    Boolean(admission.current?.enabled && admission.current.width === captured);
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
      if (current(drag.current)) setDragging(at(event.x));
    },
    onDeactivate: (event) => {
      if (!event.canceled && current(drag.current)) admission.current!.onSeek(at(event.x));
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
      if (!event.canceled && current(tapping.current)) admission.current!.onSeek(at(event.x));
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
        style={{
          flex: 1,
          minWidth: 36,
          height: 44,
          justifyContent: compact ? 'flex-start' : 'center',
          paddingTop: compact ? 1 : 0,
        }}
      >
        <View pointerEvents="none" style={{ width: '100%', height: compact ? 18 : 32 }}>
          <AudioWaveform levels={levels} progress={fraction} ink={ink} compact={compact} />
        </View>
      </View>
    </GestureDetector>
  );
}
