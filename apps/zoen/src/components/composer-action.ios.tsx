import { Host } from '@expo/ui';
import { Button } from '@expo/ui/swift-ui';
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
import { useTheme } from '../theme';
import type { ComposerActionProps } from './composer-action';

export function ComposerAction({
  name,
  label,
  onPress,
  disabled: isDisabled = false,
  prominent = false,
  testID,
  // Telegram iOS f1dd7a2, ChatTextInputPanelNode.swift:2480–2481.
  size = 40,
}: ComposerActionProps) {
  const { scheme, presentation: p } = useTheme();
  const glass = isLiquidGlassAvailable();
  return (
    <Host
      testID={testID ? `${testID}-host` : undefined}
      style={{ width: size, height: size }}
      colorScheme={scheme}
      ignoreSafeArea="keyboard"
    >
      <Button
        testID={testID}
        label={label}
        systemImage={name}
        onPress={onPress}
        modifiers={[
          buttonStyle(
            glass
              ? prominent
                ? 'glassProminent'
                : 'glass'
              : prominent
                ? 'borderedProminent'
                : 'bordered',
          ),
          buttonBorderShape('circle'),
          controlSize(size <= 44 ? 'regular' : 'large'),
          labelStyle('iconOnly'),
          tint(prominent ? p.chat.input.action : p.chat.input.control),
          foregroundStyle(prominent ? p.chat.input.actionText : p.chat.input.control),
          disabled(isDisabled),
          frame({ width: size, height: size }),
        ]}
      />
    </Host>
  );
}
