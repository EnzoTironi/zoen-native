import { useState } from 'react';
import { BottomSheet, Button, Column, Host } from '@expo/ui';
import { ComposerAction } from '../components/composer-action';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { MediaActionsProps, MediaPickKind } from './media-actions.types';

export function MediaActions({ disabled, hasVoiceEditor, onChoose }: MediaActionsProps) {
  const { scheme, colors } = useTheme();
  const [visible, setVisible] = useState(false);
  function choose(kind: MediaPickKind) {
    // Keep browser file-picker activation in the original user gesture, without an await.
    setVisible(false);
    onChoose(kind);
  }
  return (
    <>
      <ComposerAction
        testID="add-attachment"
        label={t('media.addAttachment')}
        name="plus"
        disabled={disabled}
        onPress={() => setVisible(true)}
      />
      <Host
        style={{ position: 'absolute' }}
        pointerEvents={process.env.EXPO_OS === 'web' ? 'box-none' : 'none'}
        colorScheme={scheme}
        seedColor={colors.accent}
      >
        <BottomSheet isPresented={visible} onDismiss={() => setVisible(false)}>
          <Column spacing={8}>
            <Button
              label={t('media.photosAndVideos')}
              variant="text"
              onPress={() => choose('photo')}
            />
            <Button label={t('media.camera')} variant="text" onPress={() => choose('camera')} />
            {hasVoiceEditor ? (
              <Button
                label={t('media.voiceImportAudio')}
                variant="text"
                onPress={() => choose('audio')}
              />
            ) : null}
            <Button label={t('media.file2')} variant="text" onPress={() => choose('document')} />
          </Column>
        </BottomSheet>
      </Host>
    </>
  );
}
