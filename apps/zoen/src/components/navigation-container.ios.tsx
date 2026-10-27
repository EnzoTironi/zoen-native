import { requireNativeView, requireOptionalNativeModule } from 'expo';
import type { Ref } from 'react';
import type { ConversationNativeMetrics } from '../core/conversation-reveal';
import { View } from 'react-native';
import type { NavigationContainerProps, NavigationMetricsHandle } from './navigation-container';

const available =
  requireOptionalNativeModule<{ navigationContainerVersion?: number }>('ZoenCore')
    ?.navigationContainerVersion === 1;
export const navigationMetricsAvailable =
  requireOptionalNativeModule<{ navigationMetricsVersion?: number }>('ZoenCore')
    ?.navigationMetricsVersion === 1;
const NativeContainer = available
  ? requireNativeView<
      NavigationContainerProps & {
        ref?: Ref<NavigationMetricsHandle>;
        metricsBinding?: string;
        onMetricsChanged?: (event: { nativeEvent: ConversationNativeMetrics }) => void;
      }
    >('ZoenCore', 'NavigationContainer')
  : null;

export function NavigationContainer({
  scrollViewTag,
  metrics,
  ...props
}: NavigationContainerProps) {
  return NativeContainer ? (
    <NativeContainer
      {...props}
      scrollViewTag={scrollViewTag}
      collapsable={false}
      {...(navigationMetricsAvailable
        ? {
            ref: metrics?.ref,
            metricsBinding: metrics?.binding ?? '',
            onMetricsChanged: (event: { nativeEvent: ConversationNativeMetrics }) => {
              if (event.nativeEvent.binding === metrics?.binding)
                metrics.changed(event.nativeEvent);
            },
          }
        : {})}
    />
  ) : (
    <View {...props} />
  );
}
