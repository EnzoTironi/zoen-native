import { Host } from '@expo/ui';
import {
  Button,
  DisclosureGroup,
  HStack,
  Text as NativeText,
  Slider,
  VStack,
} from '@expo/ui/swift-ui';
import {
  buttonStyle,
  disabled as disabledModifier,
  font,
  foregroundStyle,
  frame,
} from '@expo/ui/swift-ui/modifiers';
import { t } from '../i18n/index.ts';
import type { useTheme } from '../theme';
import type { projectVoiceDraft } from './voice-edit.ts';
import { voiceSelectionToEditedRange, type FrameRange } from './voice-edit.ts';
import { clamp, type Preview, type Props } from './voice-waveform-model';
type PrecisionContext = Pick<
  Props,
  'draft' | 'onEdit' | 'onPlayFromSelection' | 'auditionLabels'
> & {
  colors: ReturnType<typeof useTheme>['colors'];
  scheme: ReturnType<typeof useTheme>['scheme'];
  disabled: boolean;
  auditionDisabled: boolean;
  latestTrim(): FrameRange;
  previewTrim(range: FrameRange): void;
  previewSelection(range: FrameRange): void;
  time(frame: number): string;
  trimChanged: boolean;
  selected: FrameRange | null;
  selectionNow: { current: Preview | null };
  hasPendingTrim(): boolean;
  canonicalProjection: ReturnType<typeof projectVoiceDraft>;
  auditionReady: boolean;
  selectionPlayable: boolean;
  precision: { source: string; open: boolean };
  setPrecision(value: { source: string; open: boolean }): void;
  trim: FrameRange;
  frames: number;
  selectedRange: FrameRange;
};

/** Stateless controls; all gesture, trim and native lifetimes stay in VoiceWaveform. */
export function renderVoicePrecisionControls({
  colors,
  scheme,
  draft,
  disabled,
  auditionDisabled,
  onEdit,
  onPlayFromSelection,
  auditionLabels,
  latestTrim,
  previewTrim,
  previewSelection,
  time,
  trimChanged,
  selected,
  selectionNow,
  hasPendingTrim,
  canonicalProjection,
  auditionReady,
  selectionPlayable,
  precision,
  setPrecision,
  trim,
  frames,
  selectedRange,
}: PrecisionContext) {
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
  );
}
