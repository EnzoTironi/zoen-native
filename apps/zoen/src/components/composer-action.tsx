import { Pressable } from 'react-native';
import { useTheme } from '../theme';
import { Icon } from './primitives';

export type ComposerActionProps = {
  name:
    | 'plus'
    | 'mic'
    | 'arrow.up'
    | 'checkmark'
    | 'stop.fill'
    | 'xmark'
    | 'chevron.up'
    | 'chevron.down';
  label: string;
  onPress: () => void;
  disabled?: boolean;
  prominent?: boolean;
  testID?: string;
  size?: 44 | 52;
};
const material = {
  plus: 'add',
  mic: 'mic',
  'arrow.up': 'arrow_upward',
  checkmark: 'check',
  'stop.fill': 'stop',
  xmark: 'close',
  'chevron.up': 'expand_less',
  'chevron.down': 'expand_more',
} as const;
export function ComposerAction({
  name,
  label,
  onPress,
  disabled = false,
  prominent = false,
  testID,
  size = 52,
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
        width: size,
        height: size,
        borderRadius: size / 2,
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
