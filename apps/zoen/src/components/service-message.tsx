import { useCallback, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { Image } from 'expo-image';
import { Text, View, useWindowDimensions, type TextLayoutEvent } from 'react-native';
import { useTheme } from '../theme';
import { MessagePressable as Pressable } from './message-interaction';
import {
  telegramServiceLineRects,
  telegramServiceMask,
  type ServiceLine,
} from '../miniapps/telegram-service-message';

/** Shared Telegram action-message face. Canonical event text and actions stay with callers. */
export function ServiceMessage({
  id,
  label,
  children,
  highlighted = false,
  onPress,
  accessibilityHint,
  testID = `service-message-${id}`,
  actionTestID,
  backgroundTestID = `${testID}-background`,
  textTestID = `${testID}-text`,
}: {
  id: string;
  label: string;
  children?: ReactNode;
  highlighted?: boolean;
  onPress?: () => void;
  accessibilityHint?: string;
  testID?: string;
  actionTestID?: string;
  backgroundTestID?: string;
  textTestID?: string;
}) {
  const { colors, presentation: p } = useTheme();
  const { width, fontScale } = useWindowDimensions();
  const owner = JSON.stringify([id, label, width, fontScale]);
  const renderedOwner = useRef<string | null>(owner);
  const [measured, setMeasured] = useState<{
    owner: string;
    lines: readonly ServiceLine[];
    width: number;
  } | null>(null);
  useLayoutEffect(() => {
    renderedOwner.current = owner;
    return () => {
      renderedOwner.current = null;
    };
  }, [owner]);
  const onTextLayout = useCallback(
    (event: TextLayoutEvent) => {
      if (renderedOwner.current !== owner) return;
      const lines = event.nativeEvent.lines;
      const labelWidth = Math.max(...lines.map((line) => line.x * 2 + line.width));
      const rects = telegramServiceLineRects(lines, labelWidth);
      if (!rects.length) return;
      setMeasured((previous) =>
        previous?.owner === owner &&
        previous.width === labelWidth &&
        JSON.stringify(previous.lines) === JSON.stringify(rects)
          ? previous
          : { owner, lines: rects, width: labelWidth },
      );
    },
    [owner],
  );
  const mask =
    measured?.owner === owner
      ? telegramServiceMask(measured.lines, p.chat.service.background)
      : null;
  const faceStyle = {
    maxWidth: '100%' as const,
    paddingTop: 1,
    paddingBottom: 3,
    borderRadius: 11,
    borderWidth: highlighted ? 2 : 0,
    borderColor: colors.accent,
    transform: [{ translateX: -1 }],
  };
  const content = (
    <>
      {mask ? (
        <Image
          testID={backgroundTestID}
          source={{ uri: mask.uri }}
          contentFit="fill"
          transition={0}
          accessible={false}
          pointerEvents="none"
          style={{
            position: 'absolute',
            left: mask.x,
            top: mask.y + 1,
            width: mask.width,
            height: mask.height,
          }}
        />
      ) : null}
      <Text
        testID={textTestID}
        selectable={!onPress}
        onTextLayout={onTextLayout}
        style={{ color: p.chat.service.text, fontSize: 13, textAlign: 'center' }}
      >
        {children ?? label}
      </Text>
    </>
  );

  return (
    <View
      testID={testID}
      style={{ alignItems: 'center', width: '100%', paddingHorizontal: 16, marginVertical: 4 }}
    >
      {onPress ? (
        <Pressable
          testID={actionTestID}
          accessible
          accessibilityRole="button"
          accessibilityLabel={label}
          accessibilityHint={accessibilityHint}
          onPress={onPress}
          hitSlop={{ top: 11, bottom: 11, left: 8, right: 8 }}
          style={({ pressed }) => ({ ...faceStyle, opacity: pressed ? 0.6 : 1 })}
        >
          {content}
        </Pressable>
      ) : (
        <View
          testID={actionTestID}
          accessible
          accessibilityRole="text"
          accessibilityLabel={label}
          style={faceStyle}
        >
          {content}
        </View>
      )}
    </View>
  );
}
