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
import { useLayoutEffect, useSyncExternalStore } from 'react';
import { useAttachmentKeyboardSession } from './attachment-keyboard-context';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { MediaActionsProps } from './media-actions.types';
import { ComposerAction } from '../components/composer-action';

function MediaActionsMenu({ disabled: isDisabled, hasVoiceEditor, onChoose }: MediaActionsProps) {
  const { scheme, colors } = useTheme();
  const glass = isLiquidGlassAvailable();
  return (
    <Host
      testID="add-attachment-host"
      style={{ width: 40, height: 40 }}
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
          controlSize('regular'),
          labelStyle('iconOnly'),
          tint(colors.text),
          foregroundStyle(colors.text),
          disabled(isDisabled),
          frame({ width: 40, height: 40 }),
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

const emptyPanel = () => null;
const subscribeEmptyPanel = () => () => {};
export function MediaActions(props: MediaActionsProps) {
  const panel = useAttachmentKeyboardSession();
  const state = useSyncExternalStore(
    panel?.subscribe ?? subscribeEmptyPanel,
    panel?.getSnapshot ?? emptyPanel,
    panel?.getSnapshot ?? emptyPanel,
  );
  useLayoutEffect(
    () => panel?.register(props.onChoose, props.hasVoiceEditor, !props.disabled),
    [panel, props.onChoose, props.hasVoiceEditor, props.disabled],
  );
  if (!panel) return <MediaActionsMenu {...props} />;
  return (
    <ComposerAction
      testID="add-attachment"
      label={t(
        state?.presentation?.kind === 'attachments' ? 'common.close' : 'media.addAttachment',
      )}
      name={state?.presentation?.kind === 'attachments' ? 'xmark' : 'plus'}
      onPress={panel.openAttachments}
      disabled={
        (props.disabled && state?.presentation?.kind !== 'attachments') ||
        !state?.enabled ||
        !state.field ||
        !!state.presentation?.closing
      }
    />
  );
}
