import type { ReplySelection } from '../core/generated/ReplySelection';
import Animated, { Easing, FadeIn, FadeOut, ZoomIn, ZoomOut } from 'react-native-reanimated';
import { Text, View } from 'react-native';
import { telegramAudio } from '../core/telegram-audio-presentation';
import { t } from '../i18n/index.ts';
import { messageText } from '../core/presentation';
import type { TimelineEntry } from '../core/types';
import { useTheme } from '../theme';
import { Icon, IconButton } from './primitives';

/** The quote belongs to the composer, so keyboard and quote always move together. */
export function MessageContext({
  entry,
  editing = false,
  quote,
  onCancel,
}: {
  entry?: TimelineEntry;
  editing?: boolean;
  quote?: ReplySelection | null;
  onCancel?: () => void;
}) {
  const { presentation: p, reducedMotion } = useTheme();
  return (
    <View
      testID={editing ? 'editing-message-context' : 'reply-message-context'}
      style={{
        height: telegramAudio.accessoryHeight,
        flexDirection: 'row',
        alignItems: 'center',
        paddingLeft: telegramAudio.accessoryLeading - 8,
        paddingRight: 44,
      }}
    >
      <Animated.View
        entering={
          reducedMotion
            ? undefined
            : ZoomIn.duration(200)
                .easing(Easing.bezier(...telegramAudio.easeInOut))
                .withInitialValues({ transform: [{ scale: 0.001 }] })
        }
        exiting={
          reducedMotion
            ? undefined
            : ZoomOut.duration(200)
                .easing(Easing.bezier(...telegramAudio.easeInOut))
                .withTargetValues({ transform: [{ scale: 0.001 }] })
        }
        style={{ position: 'absolute', left: -1, top: 10, width: 44, alignItems: 'center' }}
      >
        <Icon
          name={
            editing
              ? { ios: 'pencil', android: 'edit', web: 'edit' }
              : { ios: 'arrowshape.turn.up.left', android: 'reply', web: 'reply' }
          }
          size={22}
          color={p.chat.input.action}
        />
      </Animated.View>
      <View
        style={{
          position: 'absolute',
          left: telegramAudio.accessoryLeading - 8,
          top: 8,
          width: 2,
          height: 35,
          borderRadius: 1,
          backgroundColor: p.chat.input.action,
        }}
      />
      <Animated.View
        key={`${editing ? 'edit' : 'reply'}:${entry?.id ?? 'missing'}`}
        entering={
          reducedMotion
            ? undefined
            : FadeIn.duration(300).easing(Easing.bezier(...telegramAudio.easeInOut))
        }
        exiting={
          reducedMotion
            ? undefined
            : FadeOut.duration(300).easing(Easing.bezier(...telegramAudio.easeInOut))
        }
        style={{ flex: 1, alignSelf: 'stretch', paddingLeft: 10, paddingTop: 7 }}
      >
        <Text
          numberOfLines={1}
          style={{ color: p.chat.input.action, fontSize: 15, lineHeight: 18, fontWeight: '500' }}
        >
          {editing
            ? t('chat.editMessage')
            : entry
              ? t('chat.replyingTo', { name: entry.author.name })
              : t('chat.replyingToMessage')}
        </Text>
        <Text
          numberOfLines={1}
          style={{ color: p.chat.input.placeholder, fontSize: 15, lineHeight: 18 }}
        >
          {quote && !editing
            ? quote.text
            : entry
              ? entry.voice
                ? t('media.voiceMessage')
                : messageText(entry)
              : t('chat.messageUnavailable')}
        </Text>
      </Animated.View>
      {onCancel ? (
        <View style={{ position: 'absolute', right: -8, top: 2 }}>
          <IconButton
            label={t('chat.cancel')}
            name={{ ios: 'xmark', android: 'close', web: 'close' }}
            onPress={onCancel}
          />
        </View>
      ) : null}
    </View>
  );
}
