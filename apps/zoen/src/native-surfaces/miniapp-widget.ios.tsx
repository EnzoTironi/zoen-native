import { HStack, Image, ProgressView, Text, VStack, type ImageProps } from '@expo/ui/swift-ui';
import {
  containerBackground,
  font,
  foregroundStyle,
  lineLimit,
  padding,
  privacySensitive,
  widgetURL,
} from '@expo/ui/swift-ui/modifiers';
import { createWidget, type WidgetEnvironment } from 'expo-widgets';
import type { NativeWidgetContent } from './snapshot';

/** Pure extension layout. Do not import until CNG registers the matching ZoenMiniapp kind. */
export function MiniappWidget(props: NativeWidgetContent, environment: WidgetEnvironment) {
  'widget';
  // Everything used by this runtime must come from props/environment or be declared here.
  const dark = environment.colorScheme === 'dark';
  const accent =
    environment.widgetRenderingMode === 'vibrant' || environment.isLuminanceReduced
      ? dark
        ? '#FFFFFF'
        : '#20242D'
      : props.accentHex;
  const small = environment.widgetFamily === 'systemSmall';
  const accessory = environment.widgetFamily?.startsWith('accessory') ?? false;
  return (
    <VStack
      alignment="leading"
      spacing={accessory ? 3 : 8}
      modifiers={[
        containerBackground(dark ? '#11151D' : '#F4F6FB', 'widget'),
        padding({ all: accessory ? 0 : 8 }),
        ...(props.deepLink ? [widgetURL(props.deepLink)] : []),
        privacySensitive(!props.redacted),
      ]}
    >
      <HStack spacing={6}>
        <Image systemName={props.symbol as ImageProps['systemName']} color={accent} />
        <Text modifiers={[font({ size: accessory ? 13 : 16, weight: 'semibold' }), lineLimit(2)]}>
          {props.title}
        </Text>
      </HStack>
      {props.targetMs !== undefined && !props.redacted ? (
        <Text
          timerInterval={{ lower: new Date(0), upper: new Date(props.targetMs) }}
          countsDown
          modifiers={[font({ size: accessory ? 16 : 26, weight: 'bold' }), foregroundStyle(accent)]}
        />
      ) : props.value ? (
        <Text
          modifiers={[
            font({ size: accessory ? 18 : 30, weight: 'bold' }),
            foregroundStyle(accent),
            lineLimit(1),
          ]}
        >
          {props.value}
        </Text>
      ) : null}
      {!accessory && props.eyebrow ? (
        <Text modifiers={[font({ size: 12 }), lineLimit(1)]}>{props.eyebrow}</Text>
      ) : null}
      {props.detail ? (
        <Text modifiers={[font({ size: accessory ? 11 : 13 }), lineLimit(small ? 2 : 3)]}>
          {props.detail}
        </Text>
      ) : null}
      {!accessory
        ? props.bars.slice(0, small ? 2 : 3).map((bar, index) => (
            <ProgressView key={index} value={bar.value}>
              <Text modifiers={[font({ size: 11 }), lineLimit(1)]}>{bar.label}</Text>
            </ProgressView>
          ))
        : null}
      {!accessory
        ? props.rows.slice(0, small ? 2 : 4).map((row, index) => (
            <HStack key={index} spacing={6}>
              <Image systemName={row.done ? 'checkmark.circle.fill' : 'circle'} color={accent} />
              <Text modifiers={[font({ size: 12 }), lineLimit(1)]}>{row.text}</Text>
            </HStack>
          ))
        : null}
    </VStack>
  );
}

export default createWidget<NativeWidgetContent>('ZoenMiniapp', MiniappWidget, {
  redacted: true,
  title: 'Zoen',
  detail: 'Choose a miniapp in Zoen',
  bars: [],
  rows: [],
  accentHex: '#5B8DEF',
  symbol: 'lock.fill',
});
