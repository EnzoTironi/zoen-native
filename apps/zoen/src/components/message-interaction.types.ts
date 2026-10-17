import type { ReactNode } from 'react';
import type { StyleProp, ViewStyle } from 'react-native';
export type MessageInteractionProps = Readonly<{
  messageId: string;
  mine: boolean;
  disabled?: boolean;
  onReply: () => void;
  onThread: () => void;
  /** Bubble Pressable owns these callbacks; the wrapper recognises pan only. */
  onTap?: () => void;
  onActions?: () => void;
  accessibilityLabel?: string;
  style?: StyleProp<ViewStyle>;
  children: ReactNode;
}>;
