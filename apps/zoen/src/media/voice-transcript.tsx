import { useState } from 'react';
import { Text } from 'react-native';
import Animated, { Easing, FadeIn, FadeOut, LinearTransition } from 'react-native-reanimated';
import { MessagePressable } from '../components/message-interaction';
import { Icon } from '../components/primitives';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { telegramAudio } from '../core/telegram-audio-presentation';

/** Expansion changes presentation only; the transcript is the message's accepted payload. */
export function VoiceTranscript({
  text,
  ink,
  message,
  onLongPress,
}: {
  text: string;
  ink: string;
  message: boolean;
  onLongPress?: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const { reducedMotion } = useTheme();
  return (
    <Animated.View
      testID="voice-transcript"
      layout={
        reducedMotion
          ? undefined
          : LinearTransition.duration(300).easing(Easing.bezier(...telegramAudio.easeInOut))
      }
      style={{ paddingHorizontal: message ? 14 : 0, paddingBottom: expanded && message ? 10 : 0 }}
    >
      <MessagePressable
        accessibilityRole="button"
        aria-expanded={expanded}
        accessibilityLabel={t('media.voiceTranscript')}
        onLongPress={onLongPress}
        delayLongPress={350}
        onPress={() => setExpanded((value) => !value)}
        style={{ flexDirection: 'row', alignItems: 'center', gap: 6, minHeight: 44 }}
      >
        <Icon
          name={{ ios: 'text.bubble', android: 'subtitles', web: 'subtitles' }}
          size={20}
          color={ink}
        />
        <Text style={{ flex: 1, color: ink, fontSize: 13, fontWeight: '500' }}>
          {t('media.voiceTranscript')}
        </Text>
        <Icon
          name={
            expanded
              ? { ios: 'chevron.up', android: 'expand_less', web: 'expand_less' }
              : { ios: 'chevron.down', android: 'expand_more', web: 'expand_more' }
          }
          size={11}
          color={ink}
        />
      </MessagePressable>
      {expanded ? (
        <Animated.View
          entering={
            reducedMotion
              ? undefined
              : FadeIn.duration(200).easing(Easing.bezier(...telegramAudio.easeInOut))
          }
          exiting={
            reducedMotion
              ? undefined
              : FadeOut.duration(200).easing(Easing.bezier(...telegramAudio.easeInOut))
          }
        >
          <Text selectable style={{ color: ink, fontSize: 16, lineHeight: 22 }}>
            {text}
          </Text>
        </Animated.View>
      ) : null}
    </Animated.View>
  );
}
