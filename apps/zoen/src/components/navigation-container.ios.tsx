import { requireNativeView, requireOptionalNativeModule } from 'expo';
import { View } from 'react-native';
import type { NavigationContainerProps } from './navigation-container';

const available =
  requireOptionalNativeModule<{ navigationContainerVersion?: number }>('ZoenCore')
    ?.navigationContainerVersion === 1;
const NativeContainer = available
  ? requireNativeView<NavigationContainerProps>('ZoenCore', 'NavigationContainer')
  : null;

export function NavigationContainer({ scrollViewTag, ...props }: NavigationContainerProps) {
  return NativeContainer ? (
    <NativeContainer {...props} scrollViewTag={scrollViewTag} collapsable={false} />
  ) : (
    <View {...props} />
  );
}
