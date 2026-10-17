import { BottomSheet, Button, Column, Host } from '@expo/ui';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { MediaActionsProps, MediaPickKind } from './media-actions.types';

export function MediaActions({ visible, hasVoiceEditor, onClose, onChoose }: MediaActionsProps) {
  const { scheme, colors } = useTheme();
  function choose(kind: MediaPickKind) {
    // Keep browser file-picker activation in the original user gesture, without an await.
    onClose();
    onChoose(kind);
  }
  return (
    <Host
      style={{ position: 'absolute' }}
      pointerEvents="none"
      colorScheme={scheme}
      seedColor={colors.accent}
    >
      <BottomSheet isPresented={visible} onDismiss={onClose}>
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
  );
}
