import { Host } from '@expo/ui';
import { Button, Slider, Text as NativeText, VStack } from '@expo/ui/swift-ui';
import { disabled as disabledModifier, foregroundStyle } from '@expo/ui/swift-ui/modifiers';
import { useRef, useState } from 'react';
import { Text, View } from 'react-native';
import {
  GestureDetector,
  useExclusiveGestures,
  usePanGesture,
  useTapGesture,
} from 'react-native-gesture-handler';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import {
  projectVoiceDraft,
  type FrameRange,
  type VoiceDraft,
  type VoiceEditCommand,
} from './voice-edit.ts';

type Props = {
  draft: VoiceDraft;
  onEdit: (commands: VoiceEditCommand | readonly VoiceEditCommand[]) => void;
  disabled?: boolean;
};
type Preview = { draft: VoiceDraft; range: FrameRange };
type Drag = {
  draft: VoiceDraft;
  kind: 'start' | 'end' | 'selection';
  origin: number;
  trim: FrameRange;
  selection: Preview | null;
};
const GUTTER = 22;
const clamp = (value: number, min: number, max: number) =>
  Math.max(min, Math.min(max, Math.round(value)));

/** Original-frame editor: preview locally, send one immutable edit after a completed drag. */
export function VoiceWaveform({ draft, onEdit, disabled = false }: Props) {
  const { colors, scheme } = useTheme();
  const [width, setWidth] = useState(0);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [selection, setSelection] = useState<Preview | null>(null);
  const drag = useRef<Drag | null>(null);
  const tapping = useRef<VoiceDraft | null>(null);
  const frames = draft.original.frameCount;
  const trim = preview?.draft === draft ? preview.range : draft.edit.trim;
  const selected = selection?.draft === draft ? selection.range : null;
  const track = Math.max(1, width - GUTTER * 2);
  const position = (frame: number) => GUTTER + (frame / frames) * track;
  const frameAt = (x: number) => clamp(((x - GUTTER) / track) * frames, 0, frames);
  const time = (frame: number) => `${(frame / draft.original.sampleRate).toFixed(2)} s`;
  const projection = projectVoiceDraft({ ...draft, edit: { ...draft.edit, trim } });
  const canonicalProjection = projectVoiceDraft(draft);
  const removed: FrameRange[] = [];
  let cursor = 0;
  for (const kept of projection.keptRanges) {
    if (cursor < kept.startFrame) removed.push({ startFrame: cursor, endFrame: kept.startFrame });
    cursor = kept.endFrame;
  }
  if (cursor < frames) removed.push({ startFrame: cursor, endFrame: frames });

  function updateDrag(x: number, context: Drag) {
    const frame = frameAt(x);
    const range =
      context.kind === 'start'
        ? { startFrame: Math.min(frame, context.trim.endFrame), endFrame: context.trim.endFrame }
        : context.kind === 'end'
          ? {
              startFrame: context.trim.startFrame,
              endFrame: Math.max(frame, context.trim.startFrame),
            }
          : {
              startFrame: Math.min(frame, context.origin),
              endFrame: Math.max(frame, context.origin),
            };
    if (context.kind === 'selection') setSelection({ draft, range });
    else setPreview({ draft, range });
    return range;
  }
  const pan = usePanGesture({
    enabled: !disabled && width > GUTTER * 2,
    runOnJS: true,
    activeOffsetX: [-4, 4],
    failOffsetY: [-18, 18],
    onBegin: (event) => {
      const startDistance = Math.abs(event.x - position(trim.startFrame));
      const endDistance = Math.abs(event.x - position(trim.endFrame));
      drag.current = {
        draft,
        kind:
          Math.min(startDistance, endDistance) <= GUTTER
            ? startDistance < endDistance
              ? 'start'
              : 'end'
            : 'selection',
        origin: frameAt(event.x),
        trim,
        selection,
      };
    },
    onUpdate: (event) => {
      const context = drag.current;
      if (!disabled && context?.draft === draft) updateDrag(event.x, context);
    },
    onDeactivate: (event) => {
      const context = drag.current;
      if (event.canceled || disabled || context?.draft !== draft) return;
      const range = updateDrag(event.x, context);
      drag.current = null;
      if (context.kind !== 'selection') onEdit({ type: 'trim', range });
    },
    onFinalize: () => {
      const context = drag.current;
      drag.current = null;
      if (context?.draft === draft) {
        setPreview({ draft, range: context.trim });
        setSelection(context.selection);
      }
    },
  });
  const tap = useTapGesture({
    enabled: !disabled && width > GUTTER * 2,
    runOnJS: true,
    onBegin: () => {
      tapping.current = draft;
    },
    onDeactivate: (event) => {
      if (event.canceled || disabled || tapping.current !== draft) return;
      tapping.current = null;
      const frame = Math.min(frames - 1, frameAt(event.x));
      let startFrame = 0;
      for (const part of canonicalProjection.keptRanges) {
        if (startFrame <= frame && frame < part.startFrame) {
          onEdit({ type: 'restore', range: { startFrame, endFrame: part.startFrame } });
          return;
        }
        startFrame = part.endFrame;
      }
      if (startFrame <= frame && frame < frames)
        onEdit({ type: 'restore', range: { startFrame, endFrame: frames } });
    },
    onFinalize: () => {
      tapping.current = null;
    },
  });
  const gesture = useExclusiveGestures(pan, tap);
  const bins = Math.min(draft.original.levels.length, 160, Math.floor(track / 3));
  const levels = Array.from({ length: bins }, (_, bin) => {
    const first = Math.floor((bin * draft.original.levels.length) / bins);
    const last = Math.ceil(((bin + 1) * draft.original.levels.length) / bins);
    let peak = 0;
    for (let index = first; index < last; index++)
      peak = Math.max(peak, draft.original.levels[index]!);
    return peak;
  });
  const trimChanged =
    trim.startFrame !== draft.edit.trim.startFrame || trim.endFrame !== draft.edit.trim.endFrame;
  const selectedRange = selected ?? trim;
  function slider(
    label: string,
    value: number,
    min: number,
    max: number,
    change: (value: number) => void,
  ) {
    return (
      <Slider
        label={<NativeText>{`${label}: ${time(value)}`}</NativeText>}
        value={value}
        min={min}
        max={Math.max(min + 1, max)}
        step={1}
        onValueChange={(value) => {
          if (!disabled) change(clamp(value, min, max));
        }}
        modifiers={[disabledModifier(disabled || max <= min)]}
      />
    );
  }
  return (
    <View style={{ gap: 12 }}>
      <Text style={{ color: colors.secondary, fontSize: 13 }}>{t('media.voiceWaveformHint')}</Text>
      <GestureDetector gesture={gesture}>
        <View
          collapsable={false}
          accessible
          accessibilityRole="image"
          accessibilityLabel={t('media.voiceWaveform')}
          accessibilityHint={t('media.voiceWaveformHint')}
          onLayout={(event) => setWidth(event.nativeEvent.layout.width)}
          style={{
            height: 104,
            justifyContent: 'center',
            borderRadius: 16,
            backgroundColor: colors.muted,
            overflow: 'hidden',
          }}
        >
          {removed.map((part) => (
            <View
              key={`${part.startFrame}:${part.endFrame}`}
              pointerEvents="none"
              style={{
                position: 'absolute',
                left: position(part.startFrame),
                width: ((part.endFrame - part.startFrame) / frames) * track,
                top: 22,
                height: 60,
                borderBottomWidth: 2,
                borderStyle: 'dashed',
                borderColor: colors.secondary,
                backgroundColor: colors.surface,
              }}
            />
          ))}
          <View
            pointerEvents="none"
            style={{
              marginHorizontal: GUTTER,
              height: 56,
              flexDirection: 'row',
              alignItems: 'center',
              gap: 1,
            }}
          >
            {levels.map((level, index) => {
              const frame = ((index + 0.5) / bins) * frames;
              const kept = projection.keptRanges.some(
                (part) => part.startFrame <= frame && frame < part.endFrame,
              );
              return (
                <View
                  key={index}
                  style={{
                    flex: 1,
                    height: Math.max(2, level * 52),
                    borderRadius: 2,
                    backgroundColor: kept ? colors.accent : colors.secondary,
                  }}
                />
              );
            })}
          </View>
          {selected ? (
            <View
              pointerEvents="none"
              style={{
                position: 'absolute',
                left: position(selected.startFrame),
                width: ((selected.endFrame - selected.startFrame) / frames) * track,
                top: 20,
                height: 64,
                borderWidth: 2,
                borderColor: colors.accent,
                borderRadius: 4,
              }}
            />
          ) : null}
          {[trim.startFrame, trim.endFrame].map((frame, index) => (
            <View
              key={index}
              pointerEvents="none"
              style={{
                position: 'absolute',
                left: position(frame) - 3,
                top: 12,
                width: 6,
                height: 80,
                borderRadius: 3,
                backgroundColor: colors.text,
              }}
            />
          ))}
        </View>
      </GestureDetector>
      <View style={{ flexDirection: 'row', justifyContent: 'space-between' }}>
        <Text style={{ color: colors.secondary }}>{time(trim.startFrame)}</Text>
        <Text style={{ color: colors.secondary }}>{time(trim.endFrame)}</Text>
      </View>
      <Host
        matchContents={{ vertical: true }}
        style={{ width: '100%' }}
        colorScheme={scheme}
        seedColor={colors.accent}
      >
        <VStack spacing={10}>
          {slider(t('media.voiceTrimStart'), trim.startFrame, 0, trim.endFrame, (startFrame) =>
            setPreview({ draft, range: { startFrame, endFrame: trim.endFrame } }),
          )}
          {slider(t('media.voiceTrimEnd'), trim.endFrame, trim.startFrame, frames, (endFrame) =>
            setPreview({ draft, range: { startFrame: trim.startFrame, endFrame } }),
          )}
          <Button
            label={t('media.voiceApplyTrim')}
            onPress={() => {
              if (!disabled && trimChanged) onEdit({ type: 'trim', range: trim });
            }}
            modifiers={[disabledModifier(disabled || !trimChanged), foregroundStyle(colors.accent)]}
          />
          {slider(
            t('media.voiceSelectionStart'),
            selectedRange.startFrame,
            0,
            selectedRange.endFrame,
            (startFrame) =>
              setSelection({ draft, range: { startFrame, endFrame: selectedRange.endFrame } }),
          )}
          {slider(
            t('media.voiceSelectionEnd'),
            selectedRange.endFrame,
            selectedRange.startFrame,
            frames,
            (endFrame) =>
              setSelection({ draft, range: { startFrame: selectedRange.startFrame, endFrame } }),
          )}
          <Button
            label={t('media.voiceCutSelection')}
            onPress={() => {
              if (!disabled && selected && selected.endFrame > selected.startFrame)
                onEdit({ type: 'cut', range: selected });
            }}
            modifiers={[
              disabledModifier(disabled || !selected || selected.endFrame <= selected.startFrame),
              foregroundStyle(colors.accent),
            ]}
          />
          <Button
            label={t('media.voiceRestoreSelection')}
            onPress={() => {
              if (!disabled && selected && selected.endFrame > selected.startFrame)
                onEdit({ type: 'restore', range: selected });
            }}
            modifiers={[
              disabledModifier(disabled || !selected || selected.endFrame <= selected.startFrame),
              foregroundStyle(colors.accent),
            ]}
          />
        </VStack>
      </Host>
    </View>
  );
}
