import { HStack, Image, Text, VStack } from '@expo/ui/swift-ui';
import {
  font,
  foregroundStyle,
  lineLimit,
  padding,
  privacySensitive,
} from '@expo/ui/swift-ui/modifiers';
import { createLiveActivity, type LiveActivityEnvironment } from 'expo-widgets';
import type { MiniappActivityContent } from './activity';

function MiniappActivity(props: MiniappActivityContent, environment: LiveActivityEnvironment) {
  'widget';
  const stale = environment.isStale === true;
  const privateContent = props.redacted || stale;
  const color = environment.isLuminanceReduced ? '#A6AAB1' : props.accentHex;
  const label = stale
    ? (props.labels?.refresh ?? 'Open to update')
    : props.redacted
      ? (props.labels?.countdown ?? 'Countdown in progress')
      : props.title;
  const icon = <Image systemName={privateContent ? 'lock.fill' : 'timer'} color={color} />;
  const timer = privateContent ? (
    <Text modifiers={[font({ size: 13 }), lineLimit(1)]}>
      {stale ? 'Zoen' : (props.labels?.inProgress ?? 'In progress')}
    </Text>
  ) : (
    <Text
      timerInterval={{ lower: new Date(0), upper: new Date(props.endsAt) }}
      countsDown
      modifiers={[font({ size: 22, weight: 'semibold' }), foregroundStyle(color)]}
    />
  );
  const banner = (
    <HStack spacing={12} modifiers={[padding({ all: 16 }), privacySensitive(!props.redacted)]}>
      {icon}
      <VStack alignment="leading" spacing={4}>
        <Text modifiers={[font({ size: 15, weight: 'semibold' }), lineLimit(1)]}>{label}</Text>
        {!privateContent && props.detail ? (
          <Text modifiers={[font({ size: 12 }), lineLimit(1)]}>{props.detail}</Text>
        ) : null}
      </VStack>
      {timer}
    </HStack>
  );
  return {
    banner,
    bannerSmall: (
      <HStack spacing={6}>
        {icon}
        <Text modifiers={[lineLimit(1)]}>{label}</Text>
      </HStack>
    ),
    compactLeading: icon,
    compactTrailing: timer,
    minimal: icon,
    expandedLeading: icon,
    expandedTrailing: timer,
    expandedBottom: (
      <Text modifiers={[lineLimit(2), padding({ all: 8 }), privacySensitive(!props.redacted)]}>
        {label}
      </Text>
    ),
  };
}

export default createLiveActivity('ZoenMiniappCountdown', MiniappActivity);
