import { requireNativeView } from 'expo';
import { Keyboard, type ViewProps } from 'react-native';
import { useTheme } from '../theme';
import type { KeyboardPresentation } from './attachment-keyboard-session';

export type NativeKeyboardAction = {
  token: string;
  context: string;
  action: string;
  id?: string;
  revision?: string;
  text?: string;
  ids?: string[];
};
export type NativeKeyboardClosed = { token: string; context: string; reason: string };
type Props = ViewProps & {
  binding: string;
  model: string;
  onAction: (event: { nativeEvent: NativeKeyboardAction }) => void;
  onClosed: (event: { nativeEvent: NativeKeyboardClosed }) => void;
};
export const NativeAttachmentKeyboard = requireNativeView<Props>(
  'ZoenCore',
  'ZoenKeyboardPanelHost',
);
export const NativeStandaloneAttachmentPanel = requireNativeView<Props>(
  'ZoenCore',
  'ZoenStandaloneAttachmentPanelHost',
);
export const NativeEmojiKeyboard = requireNativeView<Props>(
  'ZoenCore',
  'ZoenEmojiKeyboardPanelHost',
);
export function useKeyboardPanelBinding(
  presentation: KeyboardPresentation | null,
  active: boolean,
  closing = presentation?.closing ?? false,
) {
  const { presentation: theme } = useTheme();
  return JSON.stringify({
    token: presentation?.id ?? '',
    context: presentation?.field.context ?? '',
    tag: presentation?.field.tag ?? 0,
    label: presentation?.field.label ?? '',
    placeholder: presentation?.field.placeholder ?? '',
    active,
    closing,
    preferredHeight: Keyboard.metrics()?.height ?? 0,
    returnToKeyboard: presentation?.returnToKeyboard ?? false,
    appearance: {
      background: theme.actionSheet.background,
      text: theme.text,
      secondary: theme.secondary,
      accent: theme.accent,
      separator: theme.separator,
    },
  });
}
