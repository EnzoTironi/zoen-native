import { Pressable } from 'react-native';
import { useTheme } from '../theme';
import { Icon } from './primitives';

export type ComposerActionProps = {
  name: 'plus' | 'mic' | 'arrow.up' | 'checkmark' | 'stop.fill';
  label: string;
  onPress: () => void;
  disabled?: boolean;
  prominent?: boolean;
  testID?: string;
};
const material = {
  plus: 'add',
  mic: 'mic',
  'arrow.up': 'arrow_upward',
  checkmark: 'check',
  'stop.fill': 'stop',
} as const;
export function ComposerAction({
  name,
  label,
  onPress,
  disabled = false,
  prominent = false,
  testID,
}: ComposerActionProps) {
  const { colors } = useTheme();
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ disabled }}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => ({
        width: 52,
        height: 52,
        borderRadius: 26,
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: prominent ? colors.accent : colors.surface,
        opacity: disabled ? 0.4 : pressed ? 0.65 : 1,
      })}
    >
      <Icon
        name={{ ios: name, android: material[name], web: material[name] }}
        color={prominent ? colors.onAccent : colors.text}
      />
    </Pressable>
  );
}
