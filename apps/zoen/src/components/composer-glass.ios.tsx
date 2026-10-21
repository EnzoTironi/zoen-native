// SPDX-License-Identifier: GPL-2.0-or-later
// React/Fabric adapters for the original Telegram-iOS glass, revision f1dd7a2.
import { requireNativeView, requireOptionalNativeModule } from 'expo';
import type { ViewProps } from 'react-native';
import { useTheme } from '../theme';
import {
  composerPreferredGlass,
  composerWallpaperSaturation,
} from '../core/telegram-composer-presentation';
import {
  ComposerGlassSurface as FallbackSurface,
  ComposerGlassContainer as FallbackContainer,
  type ComposerGlassSurfaceProps,
} from './composer-glass-fallback';

const available =
  requireOptionalNativeModule<{ composerGlassVersion?: number }>('ZoenCore')
    ?.composerGlassVersion === 1;
const NativeSurface = available
  ? requireNativeView<ComposerGlassSurfaceProps & { isDark: boolean }>(
      'ZoenCore',
      'ComposerGlassSurface',
    )
  : null;
const NativeContainer = available
  ? requireNativeView<ViewProps & { isDark: boolean }>('ZoenCore', 'ComposerGlassContainer')
  : null;

export function ComposerGlassSurface({
  preference,
  interactive = true,
  ...props
}: ComposerGlassSurfaceProps) {
  const { scheme, sky } = useTheme();
  const isDark = scheme === 'dark';
  const material =
    preference ??
    composerPreferredGlass({
      dark: isDark,
      saturatedWallpaper:
        composerWallpaperSaturation(sky ? [sky.top, sky.middle, sky.bottom] : []) > 0.35,
    });
  return NativeSurface ? (
    <NativeSurface
      {...props}
      isDark={isDark}
      preference={material}
      interactive={interactive}
      collapsable={false}
    />
  ) : (
    <FallbackSurface {...props} preference={material} interactive={interactive} />
  );
}
export function ComposerGlassContainer(props: ViewProps) {
  const { scheme } = useTheme();
  return NativeContainer ? (
    <NativeContainer {...props} isDark={scheme === 'dark'} collapsable={false} />
  ) : (
    <FallbackContainer {...props} />
  );
}
