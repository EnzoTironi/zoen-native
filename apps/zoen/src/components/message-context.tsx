import type { SharedReplyDraft } from '../core/generated/SharedReplyDraft';
import type { ReplySelection } from '../core/generated/ReplySelection';
import Animated, { Easing, FadeIn, FadeOut, ZoomIn, ZoomOut } from 'react-native-reanimated';
import { Pressable, Text, View } from 'react-native';
import { telegramAudio } from '../core/telegram-audio-presentation';
import { t } from '../i18n/index.ts';
import { messageText } from '../core/presentation';
import type { TimelineEntry } from '../core/types';
import { useTheme } from '../theme';
import { Icon, IconButton } from './primitives';
import { useSemanticMessagePreview } from './use-semantic-message-preview';

/** The quote belongs to the composer, so keyboard and quote always move together. */
export function MessageContext({
  entry,
  edit,
  editing = false,
  quote,
  shared,
  onOptions,
  onCancel,
}: {
  entry?: TimelineEntry;
  edit?: { id: string; revision?: string; text: string };
  editing?: boolean;
  quote?: ReplySelection | null;
  shared?: SharedReplyDraft | null;
  onOptions?: () => void;
  onCancel?: () => void;
}) {
  const { presentation: p, reducedMotion } = useTheme();
  const literalSelection = editing ? null : quote;
  const source = edit
    ? edit.text
    : shared
      ? shared.text
      : literalSelection
        ? literalSelection.text
        : entry
          ? entry.voice
            ? t('media.voiceMessage')
            : messageText(entry)
          : t('chat.messageUnavailable');
  const preview = useSemanticMessagePreview({
    owner: JSON.stringify([
      editing,
      edit?.id ?? entry?.id,
      edit?.revision ?? entry?.text_revision,
      quote?.source_id,
      quote?.revision,
      shared?.message_id,
      shared?.revision,
    ]),
    text: source,
    literal: Boolean(
      shared ||
      literalSelection ||
      (!edit &&
        (!entry ||
          entry.voice ||
          !('Message' in entry.kind) ||
          entry.kind.Message.text.length === 0)),
    ),
  });
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
        key={`${editing ? 'edit' : 'reply'}:${shared?.message_id ?? edit?.id ?? entry?.id ?? 'missing'}`}
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
        <Pressable
          disabled={!onOptions}
          onPress={onOptions}
          accessibilityRole={onOptions ? 'button' : undefined}
          accessibilityLabel={onOptions ? t('replyOptions.title') : undefined}
        >
          <Text
            numberOfLines={1}
            style={{
              color: p.chat.input.action,
              fontSize: 15,
              lineHeight: 18,
              fontWeight: '500',
            }}
          >
            {editing
              ? t('chat.editMessage')
              : shared
                ? t('chat.replyingTo', { name: shared.author_name })
                : entry
                  ? t('chat.replyingTo', { name: entry.author.name })
                  : t('chat.replyingToMessage')}
          </Text>
          <Text
            numberOfLines={1}
            style={{ color: p.chat.input.placeholder, fontSize: 15, lineHeight: 18 }}
          >
            {preview.text ?? '…'}
          </Text>
        </Pressable>
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
