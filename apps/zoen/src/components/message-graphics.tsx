import { View } from 'react-native';
import { Icon } from './primitives';
import {
  messageBubbleGeometry,
  type MessageBubbleGraphicProps,
  type MessageReceiptGraphicProps,
} from './message-graphics-contract';

/** Deferred-platform fallback. Exact native tail/artwork lives in the iOS adapter. */
export function MessageBubbleGraphic(props: MessageBubbleGraphicProps) {
  const shape = messageBubbleGeometry(props);
  return (
    <View
      pointerEvents="none"
      accessible={false}
      style={{
        position: 'absolute',
        inset: 0,
        backgroundColor: props.fill,
        experimental_backgroundImage:
          props.fillColors && props.fillColors.length > 1
            ? `linear-gradient(to bottom, ${props.fillColors.join(', ')})`
            : undefined,
        borderTopLeftRadius: shape.topLeft,
        borderTopRightRadius: shape.topRight,
        borderBottomLeftRadius: shape.bottomLeft,
        borderBottomRightRadius: shape.bottomRight,
      }}
    />
  );
}
export function MessageReceiptGraphic({ shape, color }: MessageReceiptGraphicProps) {
  const name =
    shape === 'clock'
      ? { ios: 'clock' as const, android: 'schedule' as const, web: 'schedule' as const }
      : { ios: 'checkmark' as const, android: 'check' as const, web: 'check' as const };
  return (
    <View pointerEvents="none" accessible={false} style={{ width: 13, height: 14 }}>
      <View style={{ position: 'absolute', left: shape === 'clock' ? 1 : -4, top: 1.5 }}>
        <Icon name={name} size={11} color={color} />
      </View>
      {shape === 'double' ? (
        <View style={{ position: 'absolute', left: 2, top: 1.5 }}>
          <Icon name={name} size={11} color={color} />
        </View>
      ) : null}
    </View>
  );
}
