import { Modal } from 'react-native';
import { SkyScene } from '../components/sky-background';
import type { MediaSheetProps } from './media-sheet.types';

export function MediaSheet({ visible, locked, onDismiss, children }: MediaSheetProps) {
  return (
    <Modal
      visible={visible}
      animationType="slide"
      presentationStyle="pageSheet"
      onRequestClose={() => {
        if (!locked) onDismiss();
      }}
    >
      <SkyScene>{children}</SkyScene>
    </Modal>
  );
}
