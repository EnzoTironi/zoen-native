import type { RefObject } from 'react';
import type { ConversationNativeMetrics } from '../core/conversation-reveal';
import { View, type ViewProps } from 'react-native';

export type NavigationMetricsHandle = {
  sampleScroll(binding: string): Promise<ConversationNativeMetrics | null>;
};
export type NavigationMetricsBinding = {
  binding: string;
  ref: RefObject<NavigationMetricsHandle | null>;
  changed(packet: ConversationNativeMetrics): void;
};
export const navigationMetricsAvailable = false;
export type NavigationContainerProps = ViewProps & {
  scrollViewTag: number;
  metrics?: NavigationMetricsBinding;
};

export function NavigationContainer({
  scrollViewTag: _scrollViewTag,
  metrics: _metrics,
  ...props
}: NavigationContainerProps) {
  return <View {...props} />;
}
