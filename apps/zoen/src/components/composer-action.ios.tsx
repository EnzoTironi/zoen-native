import { Pressable, View } from 'react-native';
import { composerSendGeometry } from '../core/telegram-composer-presentation';
import { useTheme } from '../theme';
import type { ComposerActionProps } from './composer-action';
import { ComposerControlSurface, ComposerGlyph } from './composer-control-surface';

/** Telegram's send is a solid inset capsule inside a46pt slot, not a glass button. */
export function ComposerAction({
  name,
  label,
  onPress,
  disabled = false,
  prominent = false,
  testID,
  size = 40,
  minimumHeight = 40,
}: ComposerActionProps) {
  const { presentation: p } = useTheme();
  const g = composerSendGeometry(minimumHeight);
  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ disabled }}
      disabled={disabled}
      onPress={onPress}
      hitSlop={2}
      style={({ pressed }) => ({
        width: prominent ? g.slotWidth : size,
        height: prominent ? g.slotHeight : size,
        alignItems: 'center',
        justifyContent: 'center',
        opacity: disabled ? 0.4 : pressed ? 0.65 : 1,
      })}
    >
      {prominent ? (
        <View
          pointerEvents="none"
          style={{
            width: g.fillWidth,
            height: g.fillHeight,
            borderRadius: g.fillRadius,
            backgroundColor: p.chat.input.action,
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <ComposerGlyph name={name} color={p.chat.input.actionText} />
        </View>
      ) : (
        <ComposerControlSurface size={size}>
          <ComposerGlyph name={name} color={p.chat.input.panelControl} />
        </ComposerControlSurface>
      )}
    </Pressable>
  );
}
