import { useState, type ReactElement, type ReactNode } from 'react';
import { View } from 'react-native';
import Animated, {
  useAnimatedStyle,
  useSharedValue,
  type SharedValue,
} from 'react-native-reanimated';

function ComposerSpacer({
  composerHeight,
  initialHeight,
}: {
  composerHeight: SharedValue<number>;
  initialHeight: number;
}) {
  // Native shared-value writes are asynchronous; the first layout must still
  // reserve the measured height before the UI-thread mapper takes ownership.
  const spacerStyle = useAnimatedStyle(() => ({
    height: composerHeight.value || initialHeight,
  }));
  return <Animated.View pointerEvents="none" accessible={false} style={spacerStyle} />;
}

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
  const [initialComposerHeight, setInitialComposerHeight] = useState<number | null>(null);
  const composerHeight = useSharedValue(0);
  return (
    <View style={{ flex: 1, backgroundColor: 'transparent' }}>
      {initialComposerHeight !== null
        ? children(
            <ComposerSpacer
              composerHeight={composerHeight}
              initialHeight={initialComposerHeight}
            />,
          )
        : null}
      {overlay}
      <View
        pointerEvents="box-none"
        onLayout={({ nativeEvent }) => {
          const height = nativeEvent.layout.height;
          composerHeight.set(height);
          // The list's first bottom position must include the actual footer height.
          if (initialComposerHeight === null && height > 0) setInitialComposerHeight(height);
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
