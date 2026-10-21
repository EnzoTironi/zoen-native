import { View, type ViewProps } from 'react-native';
import { useTheme } from '../theme';
import type { ComposerGlassPreference } from '../core/telegram-composer-presentation';

export type ComposerGlassSurfaceProps = ViewProps & {
  cornerRadius: number;
  preference?: ComposerGlassPreference;
  interactive?: boolean;
};

/** Android/Web and an older development client keep the existing semantic fill. */
export function ComposerGlassSurface({
  cornerRadius,
  preference: _preference,
  interactive: _interactive,
  style,
  ...props
}: ComposerGlassSurfaceProps) {
  const { presentation: p } = useTheme();
  return (
    <View
      {...props}
      style={[
        {
          borderRadius: cornerRadius,
          backgroundColor: p.chat.input.fill,
          borderColor: p.chat.input.stroke,
          borderWidth: 0.5,
        },
        style,
      ]}
    />
  );
}
export function ComposerGlassContainer(props: ViewProps) {
  return <View {...props} />;
}
