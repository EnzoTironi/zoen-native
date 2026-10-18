import { Host } from '@expo/ui';
import { Button, Menu } from '@expo/ui/swift-ui';
import {
  buttonBorderShape,
  buttonStyle,
  controlSize,
  disabled,
  foregroundStyle,
  frame,
  labelStyle,
  tint,
} from '@expo/ui/swift-ui/modifiers';
import { isLiquidGlassAvailable } from 'expo-glass-effect';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { MediaActionsProps } from './media-actions.types';

export function MediaActions({
  disabled: isDisabled,
  hasVoiceEditor,
  onChoose,
}: MediaActionsProps) {
  const { scheme, colors } = useTheme();
  const glass = isLiquidGlassAvailable();
  return (
    <Host
      testID="add-attachment-host"
      style={{ width: 52, height: 52 }}
      colorScheme={scheme}
      ignoreSafeArea="keyboard"
    >
      <Menu
        testID="add-attachment"
        label={t('media.addAttachment')}
        systemImage="plus"
        modifiers={[
          buttonStyle(glass ? 'glass' : 'bordered'),
          buttonBorderShape('circle'),
          controlSize('large'),
          labelStyle('iconOnly'),
          tint(colors.text),
          foregroundStyle(colors.text),
          disabled(isDisabled),
          frame({ width: 52, height: 52 }),
        ]}
      >
        <Button
          label={t('media.photosAndVideos')}
          systemImage="photo.on.rectangle"
          onPress={() => onChoose('photo')}
        />
        <Button label={t('media.camera')} systemImage="camera" onPress={() => onChoose('camera')} />
        {hasVoiceEditor ? (
          <Button
            label={t('media.voiceImportAudio')}
            systemImage="waveform"
            onPress={() => onChoose('audio')}
          />
        ) : null}
        <Button label={t('media.file2')} systemImage="doc" onPress={() => onChoose('document')} />
      </Menu>
    </Host>
  );
}
