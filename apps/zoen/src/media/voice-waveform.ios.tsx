import { Host } from '@expo/ui';
import {
  Button,
  DisclosureGroup,
  HStack,
  Slider,
  Text as NativeText,
  VStack,
} from '@expo/ui/swift-ui';
import {
  buttonStyle,
  disabled as disabledModifier,
  font,
  foregroundStyle,
  frame,
} from '@expo/ui/swift-ui/modifiers';
import { useMemo, useRef, useState } from 'react';
import { Text, View } from 'react-native';
import {
  GestureDetector,
  useExclusiveGestures,
  usePanGesture,
  useTapGesture,
} from 'react-native-gesture-handler';
import { formatLocale, t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import {
  editedToOriginalFrame,
  originalToEditedFrame,
  projectVoiceDraft,
  voiceSelectionToEditedRange,
  type FrameRange,
  type VoiceDraft,
  type VoiceEditCommand,
} from './voice-edit.ts';

type Props = {
  draft: VoiceDraft;
  onEdit: (commands: VoiceEditCommand | readonly VoiceEditCommand[]) => void;
  onTrimPreviewChange: (draft: VoiceDraft, pending: boolean, range: FrameRange | null) => void;
  pendingTrim?: FrameRange | null;
  disabled?: boolean;
  playheadOriginalFrame?: number | null;
  onSeekOriginal?: (observedDraft: VoiceDraft, frame: number) => void;
  onPlayFromSelection?: (observedDraft: VoiceDraft, range: FrameRange) => void;
  auditionDisabled?: boolean;
  auditionLabels?: {
    position: string;
    hint: string;
    playFromSelection: string;
    seekBackward: string;
    seekForward: string;
  };
};
type Preview = { draft: VoiceDraft; range: FrameRange };
type Drag = {
  draft: VoiceDraft;
  kind: 'start' | 'end' | 'selection';
  width: number;
  origin: number;
  trim: FrameRange;
  selection: Preview | null;
};
const GUTTER = 22;
const clamp = (value: number, min: number, max: number) =>
  Math.max(min, Math.min(max, Math.round(value)));

/** Original-frame editor: preview locally, send one immutable edit after a completed drag. */
export function VoiceWaveform({
  draft,
  onEdit,
  onTrimPreviewChange,
  pendingTrim = null,
  disabled = false,
  playheadOriginalFrame = null,
  onSeekOriginal,
  onPlayFromSelection,
  auditionDisabled = false,
  auditionLabels,
}: Props) {
  const { colors, scheme } = useTheme();
  const [width, setWidth] = useState(0);
  const [preview, setPreview] = useState<Preview | null>(null);
  const previewNow = useRef<Preview | null>(null);
  const [selection, setSelection] = useState<Preview | null>(null);
  const selectionNow = useRef<Preview | null>(null);
  const [precision, setPrecision] = useState({ source: draft.original.id, open: false });
  const drag = useRef<Drag | null>(null);
  const tapping = useRef<{ draft: VoiceDraft; width: number } | null>(null);
  const frames = draft.original.frameCount;
  const trim = preview?.draft === draft ? preview.range : (pendingTrim ?? draft.edit.trim);
  const selected = selection?.draft === draft ? selection.range : null;
  const track = Math.max(1, width - GUTTER * 2);
  const position = (frame: number) => GUTTER + (frame / frames) * track;
  const frameAt = (x: number) => clamp(((x - GUTTER) / track) * frames, 0, frames);
  const timeFormatter = new Intl.NumberFormat(formatLocale, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
  const time = (frame: number) => `${timeFormatter.format(frame / draft.original.sampleRate)} s`;
  function previewTrim(range: FrameRange) {
    previewNow.current = { draft, range };
    onTrimPreviewChange(
      draft,
      range.startFrame !== draft.edit.trim.startFrame ||
        range.endFrame !== draft.edit.trim.endFrame,
      range.startFrame !== draft.edit.trim.startFrame || range.endFrame !== draft.edit.trim.endFrame
        ? range
        : null,
    );
    setPreview({ draft, range });
  }
  function latestTrim() {
    return previewNow.current?.draft === draft
      ? previewNow.current.range
      : (pendingTrim ?? draft.edit.trim);
  }
  function hasPendingTrim() {
    const range = latestTrim();
    return (
      range.startFrame !== draft.edit.trim.startFrame || range.endFrame !== draft.edit.trim.endFrame
    );
  }
  function previewSelection(range: FrameRange) {
    selectionNow.current = { draft, range };
    setSelection({ draft, range });
  }
  const projection = useMemo(
    () =>
      projectVoiceDraft({
        ...draft,
        edit: { ...draft.edit, trim: { startFrame: trim.startFrame, endFrame: trim.endFrame } },
      }),
    [draft, trim.startFrame, trim.endFrame],
  );
  const canonicalProjection = useMemo(() => projectVoiceDraft(draft), [draft]);
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
    if (context.kind === 'selection') previewSelection(range);
    else previewTrim(range);
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
        width,
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
      if (!disabled && context?.draft === draft && context.width === width)
        updateDrag(event.x, context);
    },
    onDeactivate: (event) => {
      const context = drag.current;
      if (event.canceled || disabled || context?.draft !== draft || context.width !== width) return;
      const range = updateDrag(event.x, context);
      drag.current = null;
      if (context.kind !== 'selection') onEdit({ type: 'trim', range });
    },
    onFinalize: () => {
      const context = drag.current;
      drag.current = null;
      if (context?.draft === draft) {
        previewTrim(context.trim);
        selectionNow.current = context.selection;
        setSelection(context.selection);
      }
    },
  });
  const tap = useTapGesture({
    enabled: !disabled && width > GUTTER * 2,
    runOnJS: true,
    onBegin: () => {
      tapping.current = { draft, width };
    },
    onDeactivate: (event) => {
      if (
        event.canceled ||
        disabled ||
        hasPendingTrim() ||
        tapping.current?.draft !== draft ||
        tapping.current.width !== width
      )
        return;
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
      else if (!auditionDisabled) onSeekOriginal?.(draft, frame);
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
  const playhead =
    playheadOriginalFrame !== null && Number.isFinite(playheadOriginalFrame)
      ? Math.max(0, Math.min(frames, playheadOriginalFrame))
      : null;
  const auditionReady =
    !disabled && !auditionDisabled && !trimChanged && canonicalProjection.frameCount > 0;
  const seekable = Boolean(onSeekOriginal && auditionLabels);
  const selectionPlayable =
    selected !== null && voiceSelectionToEditedRange(canonicalProjection, selected) !== null;
  function slider(
    label: string,
    value: number,
    min: number,
    max: number,
    change: 'start' | 'end' | 'selectionStart' | 'selectionEnd',
  ) {
    return (
      <VStack alignment="leading" spacing={4}>
        <NativeText
          modifiers={[font({ textStyle: 'subheadline' }), foregroundStyle(colors.secondary)]}
        >
          {`${label}: ${time(value)}`}
        </NativeText>
        <Slider
          label={<NativeText>{`${label}: ${time(value)}`}</NativeText>}
          // A source can contain millions of frames. A stepped SwiftUI slider
          // creates native tick views for those steps; keep the native domain
          // continuous and bounded, then round only at the original-frame boundary.
          value={max > min ? (value - min) / (max - min) : 0}
          min={0}
          max={1}
          onValueChange={(fraction) => {
            if (disabled || !Number.isFinite(fraction)) return;
            const value = clamp(min + Math.max(0, Math.min(1, fraction)) * (max - min), min, max);
            if (change === 'selectionStart' || change === 'selectionEnd') {
              const current =
                selectionNow.current?.draft === draft ? selectionNow.current.range : selectedRange;
              previewSelection(
                change === 'selectionStart'
                  ? { startFrame: Math.min(value, current.endFrame), endFrame: current.endFrame }
                  : {
                      startFrame: current.startFrame,
                      endFrame: Math.max(current.startFrame, value),
                    },
              );
            } else {
              const current = latestTrim();
              previewTrim(
                change === 'start'
                  ? { startFrame: Math.min(value, current.endFrame), endFrame: current.endFrame }
                  : {
                      startFrame: current.startFrame,
                      endFrame: Math.max(current.startFrame, value),
                    },
              );
            }
          }}
          modifiers={[disabledModifier(disabled || max <= min)]}
        />
      </VStack>
    );
  }
  return (
    <View style={{ gap: 12 }}>
      <Text
        accessibilityRole="header"
        style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}
      >
        {t('media.voiceWaveform')}
      </Text>
      <Text style={{ color: colors.secondary, fontSize: 13 }}>
        {seekable ? auditionLabels!.hint : t('media.voiceWaveformHint')}
      </Text>
      <GestureDetector gesture={gesture}>
        <View
          collapsable={false}
          accessible
          accessibilityRole={seekable ? 'adjustable' : 'image'}
          accessibilityLabel={seekable ? auditionLabels!.position : t('media.voiceWaveform')}
          accessibilityHint={seekable ? auditionLabels!.hint : t('media.voiceWaveformHint')}
          accessibilityState={seekable ? { disabled: !auditionReady } : undefined}
          accessibilityValue={
            seekable
              ? {
                  min: 0,
                  max: frames,
                  now: playhead ?? canonicalProjection.keptRanges[0]?.startFrame ?? 0,
                  text: `${time(playhead ?? canonicalProjection.keptRanges[0]?.startFrame ?? 0)} / ${time(frames)}`,
                }
              : undefined
          }
          accessibilityActions={
            seekable
              ? [
                  { name: 'decrement', label: auditionLabels!.seekBackward },
                  { name: 'increment', label: auditionLabels!.seekForward },
                ]
              : undefined
          }
          onAccessibilityAction={(event) => {
            if (disabled || auditionDisabled || hasPendingTrim() || !onSeekOriginal) return;
            const action = event.nativeEvent.actionName;
            if (action !== 'increment' && action !== 'decrement') return;
            const current = originalToEditedFrame(
              canonicalProjection,
              playhead ?? canonicalProjection.keptRanges[0]?.startFrame ?? 0,
            );
            if (current === null) return;
            const frame = editedToOriginalFrame(
              canonicalProjection,
              current + (action === 'increment' ? 5 : -5) * draft.original.sampleRate,
            );
            if (frame !== null) onSeekOriginal(draft, frame);
          }}
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
          {playhead !== null && auditionReady ? (
            <View
              testID="voice-editor-playhead"
              pointerEvents="none"
              style={{
                position: 'absolute',
                left: position(playhead) - 1,
                top: 12,
                width: 2,
                height: 80,
                borderRadius: 1,
                backgroundColor: colors.text,
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
        testID="voice-precision-host"
        matchContents={{ vertical: true }}
        style={{ width: '100%' }}
        colorScheme={scheme}
        seedColor={colors.accent}
      >
        <VStack alignment="leading" spacing={12}>
          {trimChanged ? (
            <HStack spacing={8}>
              <Button
                label={t('media.voiceApplyTrim')}
                onPress={() => {
                  const range = latestTrim();
                  if (
                    !disabled &&
                    (range.startFrame !== draft.edit.trim.startFrame ||
                      range.endFrame !== draft.edit.trim.endFrame)
                  )
                    onEdit({ type: 'trim', range });
                }}
                modifiers={[
                  buttonStyle('bordered'),
                  frame({ minHeight: 44 }),
                  disabledModifier(disabled),
                  foregroundStyle(colors.accent),
                ]}
              />
              <Button
                label={t('media.voiceDiscardTrim')}
                onPress={() => {
                  if (!disabled) previewTrim(draft.edit.trim);
                }}
                modifiers={[
                  buttonStyle('bordered'),
                  frame({ minHeight: 44 }),
                  disabledModifier(disabled),
                  foregroundStyle(colors.text),
                ]}
              />
            </HStack>
          ) : null}
          {selected ? (
            <VStack alignment="leading" spacing={8}>
              {onPlayFromSelection && auditionLabels ? (
                <Button
                  label={auditionLabels.playFromSelection}
                  systemImage="play.fill"
                  onPress={() => {
                    const range =
                      selectionNow.current?.draft === draft ? selectionNow.current.range : selected;
                    if (
                      !disabled &&
                      !auditionDisabled &&
                      !hasPendingTrim() &&
                      voiceSelectionToEditedRange(canonicalProjection, range)
                    )
                      onPlayFromSelection(draft, range);
                  }}
                  modifiers={[
                    buttonStyle('bordered'),
                    frame({ minHeight: 44 }),
                    disabledModifier(!auditionReady || !selectionPlayable),
                    foregroundStyle(colors.accent),
                  ]}
                />
              ) : null}
              <Button
                label={t('media.voiceCutSelection')}
                onPress={() => {
                  if (!disabled && !trimChanged && selected.endFrame > selected.startFrame)
                    onEdit({ type: 'cut', range: selected });
                }}
                modifiers={[
                  buttonStyle('bordered'),
                  frame({ minHeight: 44 }),
                  disabledModifier(
                    disabled || trimChanged || selected.endFrame <= selected.startFrame,
                  ),
                  foregroundStyle(colors.accent),
                ]}
              />
              <Button
                label={t('media.voiceRestoreSelection')}
                onPress={() => {
                  if (!disabled && !trimChanged && selected.endFrame > selected.startFrame)
                    onEdit({ type: 'restore', range: selected });
                }}
                modifiers={[
                  buttonStyle('bordered'),
                  frame({ minHeight: 44 }),
                  disabledModifier(
                    disabled || trimChanged || selected.endFrame <= selected.startFrame,
                  ),
                  foregroundStyle(colors.accent),
                ]}
              />
            </VStack>
          ) : null}
          <DisclosureGroup
            testID="voice-precision"
            isExpanded={precision.source === draft.original.id && precision.open}
            onIsExpandedChange={(open) => setPrecision({ source: draft.original.id, open })}
          >
            <DisclosureGroup.Label>
              <NativeText
                modifiers={[
                  font({ textStyle: 'subheadline' }),
                  foregroundStyle(colors.secondary),
                  frame({ minHeight: 44, alignment: 'leading' }),
                ]}
              >
                {t('media.voicePrecisionAdjustments')}
              </NativeText>
            </DisclosureGroup.Label>
            <VStack alignment="leading" spacing={20}>
              <VStack alignment="leading" spacing={16}>
                <NativeText
                  modifiers={[font({ textStyle: 'headline' }), foregroundStyle(colors.text)]}
                >
                  {t('media.voiceTrimGroup')}
                </NativeText>
                {slider(t('media.voiceTrimStart'), trim.startFrame, 0, trim.endFrame, 'start')}
                {slider(t('media.voiceTrimEnd'), trim.endFrame, trim.startFrame, frames, 'end')}
              </VStack>
              <VStack alignment="leading" spacing={16}>
                <NativeText
                  modifiers={[font({ textStyle: 'headline' }), foregroundStyle(colors.text)]}
                >
                  {t('media.voiceSelectionGroup')}
                </NativeText>
                {slider(
                  t('media.voiceSelectionStart'),
                  selectedRange.startFrame,
                  0,
                  selectedRange.endFrame,
                  'selectionStart',
                )}
                {slider(
                  t('media.voiceSelectionEnd'),
                  selectedRange.endFrame,
                  selectedRange.startFrame,
                  frames,
                  'selectionEnd',
                )}
              </VStack>
            </VStack>
          </DisclosureGroup>
        </VStack>
      </Host>
    </View>
  );
}
