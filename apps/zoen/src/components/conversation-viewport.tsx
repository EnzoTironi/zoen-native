import { useState, type ReactNode } from 'react';
import { View } from 'react-native';

/** Keep messages behind the composer while its measured height protects the latest message. */
export function ConversationViewport({
  children,
  composer,
  overlay,
}: {
  children: (composerHeight: number) => ReactNode;
  composer: ReactNode;
  overlay?: ReactNode;
}) {
  const [composerHeight, setComposerHeight] = useState(0);
  return (
    <View style={{ flex: 1, backgroundColor: 'transparent' }}>
      {children(composerHeight)}
      {overlay}
      <View
        pointerEvents="box-none"
        onLayout={({ nativeEvent }) => setComposerHeight(nativeEvent.layout.height)}
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
