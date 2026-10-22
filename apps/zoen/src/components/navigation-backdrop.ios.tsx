import { requireNativeView, requireOptionalNativeModule } from 'expo';
import type { ViewProps } from 'react-native';
import { useTheme } from '../theme';
import type { NavigationBackdropProps } from './navigation-backdrop';

const available =
  requireOptionalNativeModule<{ navigationBackdropVersion?: number }>('ZoenCore')
    ?.navigationBackdropVersion === 1;
const NativeBackdrop = available
  ? requireNativeView<ViewProps & { isDark: boolean; solidHeight: number }>(
      'ZoenCore',
      'NavigationBackdrop',
    )
  : null;

export function NavigationBackdrop({ height, solidHeight }: NavigationBackdropProps) {
  const { scheme } = useTheme();
  // Old development clients cannot render this new native view. Keep them
  // loadable until rebuilt; do not substitute a solid strip for actual blur.
  return NativeBackdrop ? (
    <NativeBackdrop
      testID="navigation-backdrop"
      pointerEvents="none"
      accessible={false}
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      collapsable={false}
      isDark={scheme === 'dark'}
      solidHeight={solidHeight}
      style={{ position: 'absolute', top: 0, left: 0, right: 0, height }}
    />
  ) : null;
}
