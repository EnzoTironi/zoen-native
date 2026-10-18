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
  size = 52,
}: ComposerActionProps) {
  const { colors, scheme } = useTheme();
  const glass = isLiquidGlassAvailable();
  return (
    <Host style={{ width: size, height: size }} colorScheme={scheme} ignoreSafeArea="keyboard">
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
          controlSize(size === 44 ? 'regular' : 'large'),
          labelStyle('iconOnly'),
          tint(prominent ? colors.accent : colors.text),
          foregroundStyle(prominent ? colors.onAccent : colors.text),
          disabled(isDisabled),
          frame({ width: size, height: size }),
        ]}
      />
    </Host>
  );
}
