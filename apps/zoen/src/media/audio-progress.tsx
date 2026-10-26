import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import { audioWaveformBars, telegramAudio } from '../core/telegram-audio-presentation';
import { Text, View } from 'react-native';
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
  sourceIdentity,
  levels,
  progress,
  duration,
  ink,
  compact,
  disabled,
  onSeek,
}: {
  sourceIdentity: unknown;
  levels: readonly number[];
  progress: number;
  duration: number;
  ink: string;
  compact?: boolean;
  disabled: boolean;
  onSeek: (fraction: number) => void;
}) {
  const [width, setWidth] = useState(0);
  const [dragging, setDragging] = useState<{ owner: object; fraction: number } | null>(null);
  const drag = useRef<object | null>(null);
  const tapping = useRef<object | null>(null);
  const messagePan = useMessagePan();
  const consumePress = useMessagePressConsumption();
  const enabled = !disabled && duration > 0 && width > 0;
  // Ordinary progress/callback updates retain a pointer. Replacing the source,
  // geometry or admission retires it, including A → B → A source reuse.
  const owner = useMemo(
    () => ({ sourceIdentity, width, duration, enabled }),
    [sourceIdentity, width, duration, enabled],
  );
  const admission = useRef<{ owner: object; enabled: boolean; onSeek: typeof onSeek } | null>(null);
  useLayoutEffect(() => {
    admission.current = { owner, enabled, onSeek };
    return () => {
      admission.current = null;
    };
  }, [owner, enabled, onSeek]);
  const at = (x: number) => clamp(x / Math.max(1, width));
  const current = (captured: object | null) =>
    Boolean(captured === owner && admission.current?.enabled && admission.current.owner === owner);
  const pan = usePanGesture({
    enabled,
    runOnJS: true,
    activeOffsetX: [-4, 4],
    failOffsetY: [-12, 12],
    block: messagePan,
    onBegin: () => {
      if (current(owner)) drag.current = owner;
    },
    onActivate: () => {
      if (current(drag.current)) consumePress();
    },
    onUpdate: (event) => {
      if (current(drag.current)) setDragging({ owner, fraction: at(event.x) });
    },
    onDeactivate: (event) => {
      if (!event.canceled && current(drag.current)) admission.current!.onSeek(at(event.x));
    },
    onFinalize: () => {
      if (drag.current === owner) drag.current = null;
      setDragging((value) => (value?.owner === owner ? null : value));
    },
  });
  const tap = useTapGesture({
    enabled,
    runOnJS: true,
    block: messagePan,
    onBegin: () => {
      if (current(owner)) tapping.current = owner;
    },
    onActivate: () => {
      if (current(tapping.current)) consumePress();
    },
    onDeactivate: (event) => {
      if (!event.canceled && current(tapping.current)) admission.current!.onSeek(at(event.x));
    },
    onFinalize: () => {
      if (tapping.current === owner) tapping.current = null;
    },
  });
  const gesture = useExclusiveGestures(pan, tap);
  const scrubbing = dragging?.owner === owner;
  const fraction = clamp(scrubbing ? dragging.fraction : progress);
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
          height: compact ? 44 : 60,
          justifyContent: compact ? 'flex-start' : 'center',
          paddingTop: compact ? 1 : 0,
        }}
      >
        <View pointerEvents="none" style={{ width: '100%', height: compact ? 18 : 32 }}>
          <AudioWaveform levels={levels} progress={fraction} ink={ink} compact={compact} />
        </View>
        <Text
          pointerEvents="none"
          style={{
            position: compact ? 'absolute' : undefined,
            bottom: compact ? 2 : undefined,
            left: compact ? 0 : undefined,
            color: ink,
            opacity: 0.72,
            fontSize: compact ? 13 : 12,
            lineHeight: compact ? 16 : undefined,
            fontVariant: ['tabular-nums'],
          }}
        >
          {time(scrubbing || (fraction > 0 && fraction < 1) ? fraction * duration : duration)}
        </Text>
      </View>
    </GestureDetector>
  );
}
