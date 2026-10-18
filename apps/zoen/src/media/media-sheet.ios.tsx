import { BottomSheet, Host } from '@expo/ui';
import { RNHostView } from '@expo/ui/swift-ui';
import { interactiveDismissDisabled } from '@expo/ui/swift-ui/modifiers';
import { SkyScene } from '../components/sky-background';
import { useTheme } from '../theme';
import type { MediaSheetProps } from './media-sheet.types';

export function MediaSheet({ visible, locked, onDismiss, children }: MediaSheetProps) {
  const { scheme, colors } = useTheme();
  return (
    <Host
      style={{ position: 'absolute' }}
      pointerEvents="none"
      colorScheme={scheme}
      seedColor={colors.accent}
    >
      <BottomSheet
        isPresented={visible}
        onDismiss={onDismiss}
        snapPoints={['full']}
        contentPadding={0}
        containerColor="transparent"
        modifiers={[interactiveDismissDisabled(locked)]}
      >
        <RNHostView>
          <SkyScene>{children}</SkyScene>
        </RNHostView>
      </BottomSheet>
    </Host>
  );
}
