import type { ReactElement, ReactNode } from 'react';
import { View } from 'react-native';
import Animated, { useAnimatedStyle, useSharedValue } from 'react-native-reanimated';

/** Keep messages behind the composer while its measured height protects the latest message. */
export function ConversationViewport({
  children,
  composer,
  overlay,
}: {
  children: (composerSpacer: ReactElement) => ReactNode;
  composer: ReactNode;
  overlay?: ReactNode;
}) {
  const composerHeight = useSharedValue(0);
  const spacerStyle = useAnimatedStyle(() => ({ height: composerHeight.value }));
  return (
    <View style={{ flex: 1, backgroundColor: 'transparent' }}>
      {children(<Animated.View pointerEvents="none" accessible={false} style={spacerStyle} />)}
      {overlay}
      <View
        pointerEvents="box-none"
        onLayout={({ nativeEvent }) => {
          composerHeight.set(nativeEvent.layout.height);
        }}
        style={{
          position: 'absolute',
          left: 0,
          right: 0,
          bottom: 0,
          zIndex: 2,
          backgroundColor: 'transparent',
        }}
      >
        {composer}
      </View>
    </View>
  );
}
