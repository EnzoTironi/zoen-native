import { useLayoutEffect, useRef } from 'react';
import { Host } from '@expo/ui';
import { BottomSheet, Button, Group, VStack } from '@expo/ui/swift-ui';
import {
  buttonStyle,
  foregroundStyle,
  frame,
  padding,
  preferredColorScheme,
  presentationDragIndicator,
} from '@expo/ui/swift-ui/modifiers';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { MediaActionsProps, MediaPickKind } from './media-actions.types';

/** Picker presentation belongs to the fully-dismissed native sheet callback. */
export function MediaActions({ visible, hasVoiceEditor, onClose, onChoose }: MediaActionsProps) {
  const { scheme, colors } = useTheme();
  const pending = useRef<MediaPickKind | null>(null);
  const mounted = useRef(false);
  useLayoutEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      pending.current = null;
    };
  }, []);
  useLayoutEffect(() => {
    // A new presentation never inherits an unconsumed choice from a prior one.
    if (visible) pending.current = null;
  }, [visible]);
  function cancel() {
    pending.current = null;
    if (mounted.current) onClose();
  }
  function choose(kind: MediaPickKind) {
    if (!mounted.current || !visible || pending.current) return;
    pending.current = kind;
    onClose();
  }
  const actionStyle = [
    buttonStyle('plain'),
    foregroundStyle(colors.text),
    frame({ minHeight: 48, maxWidth: Infinity, alignment: 'leading' }),
  ];
  return (
    <Host
      style={{ position: 'absolute' }}
      pointerEvents="none"
      colorScheme={scheme}
      seedColor={colors.accent}
    >
      <BottomSheet
        isPresented={visible}
        fitToContents
        onIsPresentedChange={(presented) => {
          if (!presented && mounted.current) onClose();
        }}
        onDismiss={() => {
          // SDK58's direct SwiftUI callback runs after UIKit finishes dismissal.
          // Clear first so duplicate callbacks cannot open a second picker.
          const kind = pending.current;
          pending.current = null;
          if (mounted.current && !visible && kind) onChoose(kind);
        }}
      >
        <Group modifiers={[preferredColorScheme(scheme), presentationDragIndicator('visible')]}>
          <VStack spacing={8} modifiers={[padding({ all: 20 })]}>
            <Button
              label={t('media.photosAndVideos')}
              systemImage="photo.on.rectangle"
              onPress={() => choose('photo')}
              modifiers={actionStyle}
            />
            <Button
              label={t('media.camera')}
              systemImage="camera"
              onPress={() => choose('camera')}
              modifiers={actionStyle}
            />
            {hasVoiceEditor ? (
              <Button
                label={t('media.voiceImportAudio')}
                systemImage="waveform"
                onPress={() => choose('audio')}
                modifiers={actionStyle}
              />
            ) : null}
            <Button
              label={t('media.file2')}
              systemImage="doc"
              onPress={() => choose('document')}
              modifiers={actionStyle}
            />
            <Button
              label={t('media.cancelAttachment')}
              onPress={cancel}
              modifiers={[frame({ minHeight: 44 })]}
            />
          </VStack>
        </Group>
      </BottomSheet>
    </Host>
  );
}
