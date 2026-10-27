import { Text } from '@expo/ui';
import { frame } from '@expo/ui/swift-ui/modifiers';

/** SwiftUI paints the button style around its label, not an outer Button frame. */
export function FullWidthButtonLabel({ label }: { label: string }) {
  return (
    <Text
      modifiers={[frame({ maxWidth: Infinity, minHeight: 44 })]}
      textStyle={{ textAlign: 'center' }}
    >
      {label}
    </Text>
  );
}
