import { View, type ViewProps } from 'react-native';

export type NavigationContainerProps = ViewProps & { scrollViewTag: number };

export function NavigationContainer({
  scrollViewTag: _scrollViewTag,
  ...props
}: NavigationContainerProps) {
  return <View {...props} />;
}
