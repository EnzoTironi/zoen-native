import type { ReactNode } from 'react';
import type { StyleProp, ViewStyle } from 'react-native';
export type MessageInteractionProps = Readonly<{
  messageId: string;
  mine: boolean;
  disabled?: boolean;
  onReply: () => void;
  onThread: () => void;
  /** Bubble MessagePressable owns physical callbacks; the wrapper recognises pan only. */
  onTap?: () => void;
  onActions?: () => void;
  actionsAccessibilityLabel?: string;
  onQuote?: () => void;
  accessibilityLabel?: string;
  style?: StyleProp<ViewStyle>;
  children: ReactNode;
}>;
