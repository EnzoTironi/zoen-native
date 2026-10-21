import { requireNativeView } from 'expo';
import type { ViewProps } from 'react-native';
import {
  messageBubbleGeometry,
  type MessageBubbleGraphicProps,
  type MessageReceiptGraphicProps,
} from './message-graphics-contract';

type NativeBubbleProps = ViewProps &
  ReturnType<typeof messageBubbleGeometry> & {
    ownerKey: string;
    incoming: boolean;
    fill: string;
    fillColors: readonly string[];
    animateChanges: boolean;
  };
const NativeBubble = requireNativeView<NativeBubbleProps>('ZoenCore', 'MessageBubble');
const NativeReceipt = requireNativeView<ViewProps & MessageReceiptGraphicProps>(
  'ZoenCore',
  'MessageReceipt',
);

export function MessageBubbleGraphic(props: MessageBubbleGraphicProps) {
  return (
    <NativeBubble
      {...messageBubbleGeometry(props)}
      ownerKey={props.ownerKey}
      incoming={props.incoming}
      fill={props.fill}
      fillColors={props.fillColors ?? [props.fill]}
      animateChanges={props.animateChanges === true}
      pointerEvents="none"
      accessible={false}
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={{ position: 'absolute', inset: 0 }}
    />
  );
}
export function MessageReceiptGraphic(props: MessageReceiptGraphicProps) {
  return (
    <NativeReceipt
      {...props}
      pointerEvents="none"
      accessible={false}
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={{ width: 13, height: 14 }}
    />
  );
}
