import {
  createContext,
  forwardRef,
  use,
  useCallback,
  useMemo,
  useState,
  type ReactElement,
  type ReactNode,
} from 'react';
import { View, type ScrollViewProps, type StyleProp, type ViewStyle } from 'react-native';
import {
  KeyboardAvoidingView,
  KeyboardChatScrollView,
  KeyboardStickyView,
  type KeyboardChatScrollViewRef,
} from 'react-native-keyboard-controller';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import Animated, {
  useAnimatedStyle,
  useSharedValue,
  type SharedValue,
} from 'react-native-reanimated';

type NativeScrollContextValue = {
  padding: SharedValue<number>;
  offset: number;
  onInitialInset?: (height: number) => void;
};
const NativeScrollContext = createContext<NativeScrollContextValue | null>(null);

/** Forward the list's native ref and events through the supported chat scroller. */
const NativeConversationScrollView = forwardRef<KeyboardChatScrollViewRef, ScrollViewProps>(
  (props, ref) => {
    const context = use(NativeScrollContext);
    if (!context) throw new Error('ConversationViewport is required.');
    const onInset = useCallback(
      ({ bottom }: { bottom: number }) => {
        if (!Number.isFinite(bottom) || bottom <= 0) return;
        // The SDK produced a positive reservation. Initial positioning also
        // waits for loaded rows, native content measurement and its next frame.
        context.onInitialInset?.(bottom);
      },
      [context],
    );
    return (
      <KeyboardChatScrollView
        {...props}
        ref={ref}
        testID="conversation-timeline"
        automaticallyAdjustContentInsets={false}
        automaticallyAdjustKeyboardInsets={false}
        contentInsetAdjustmentBehavior="never"
        keyboardDismissMode="interactive"
        offset={context.offset}
        extraContentPadding={context.padding}
        onContentInsetChange={context.onInitialInset ? onInset : undefined}
      />
    );
  },
);

type ViewportProps = {
  children: (
    composerSpacer: ReactElement | undefined,
    ScrollComponent?: typeof NativeConversationScrollView,
  ) => ReactNode;
  composer: ReactNode;
  overlay?: ReactNode;
  initialFooterKey?: string;
  onInitialFooterLayout?: (height: number) => void;
};

/** iOS owns keyboard insets natively; other platforms retain their existing layout. */
export function ConversationContainer({
  children,
  style,
}: {
  children: ReactNode;
  style?: StyleProp<ViewStyle>;
}) {
  return process.env.EXPO_OS === 'ios' ? (
    <View style={style}>{children}</View>
  ) : (
    <KeyboardAvoidingView behavior="padding" automaticOffset style={style}>
      {children}
    </KeyboardAvoidingView>
  );
}

function NativeConversationViewport({
  children,
  composer,
  overlay,
  initialFooterKey,
  onInitialFooterLayout,
}: ViewportProps) {
  const [measured, setMeasured] = useState(false);
  const composerHeight = useSharedValue(0);
  const { bottom } = useSafeAreaInsets();
  const correction = Math.max(bottom, 12) - 8;
  // The composer keeps a stable layout. Its safe-area adjustment is a transform,
  // so native inset/offset updates never wait for a JS onLayout on keyboard frames.
  // Subtract the same adjustment from the keyboard contribution, leaving the
  // full measured composer reservation constant throughout keyboard gestures.
  const context = useMemo(
    () => ({ padding: composerHeight, offset: correction, onInitialInset: onInitialFooterLayout }),
    [composerHeight, correction, onInitialFooterLayout],
  );
  return (
    <View style={{ flex: 1, backgroundColor: 'transparent' }}>
      <NativeScrollContext key={initialFooterKey} value={context}>
        {measured ? children(undefined, NativeConversationScrollView) : null}
      </NativeScrollContext>
      {overlay}
      <KeyboardStickyView
        testID="conversation-composer"
        pointerEvents="box-none"
        offset={{ opened: correction }}
        style={{ position: 'absolute', left: 0, right: 0, bottom: 0, zIndex: 2 }}
      >
        <View
          pointerEvents="box-none"
          onLayout={({ nativeEvent }) => {
            const height = nativeEvent.layout.height;
            if (!Number.isFinite(height) || height <= 0) return;
            composerHeight.set(height);
            if (!measured) setMeasured(true);
          }}
        >
          {composer}
        </View>
      </KeyboardStickyView>
    </View>
  );
}

function ComposerSpacer({
  composerHeight,
  initialHeight,
  onInitialLayout,
}: {
  composerHeight: SharedValue<number>;
  initialHeight: number;
  onInitialLayout?: (height: number) => void;
}) {
  const [reported, setReported] = useState(false);
  // Native shared-value writes are asynchronous; the first layout must still
  // reserve the measured height before the UI-thread mapper takes ownership.
  const spacerStyle = useAnimatedStyle(() => ({
    height: composerHeight.value || initialHeight,
  }));
  return (
    <Animated.View
      pointerEvents="none"
      accessible={false}
      style={spacerStyle}
      onLayout={
        !reported && onInitialLayout
          ? ({ nativeEvent }) => {
              const height = nativeEvent.layout.height;
              if (height <= 0) return;
              onInitialLayout(height);
              setReported(true);
            }
          : undefined
      }
    />
  );
}

/** Keep messages behind the composer while its measured height protects the latest message. */
function FallbackConversationViewport({
  children,
  composer,
  overlay,
  initialFooterKey,
  onInitialFooterLayout,
}: ViewportProps) {
  const [initialComposerHeight, setInitialComposerHeight] = useState<number | null>(null);
  const composerHeight = useSharedValue(0);
  return (
    <View style={{ flex: 1, backgroundColor: 'transparent' }}>
      {initialComposerHeight !== null
        ? children(
            <ComposerSpacer
              key={initialFooterKey}
              composerHeight={composerHeight}
              initialHeight={initialComposerHeight}
              onInitialLayout={onInitialFooterLayout}
            />,
          )
        : null}
      {overlay}
      <View
        testID="conversation-composer"
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

export function ConversationViewport(props: ViewportProps) {
  return process.env.EXPO_OS === 'ios' ? (
    <NativeConversationViewport {...props} />
  ) : (
    <FallbackConversationViewport {...props} />
  );
}
