import { Text, View } from 'react-native';
import { MessageActions } from './message-actions';
import { MessageMarkdown } from './message-markdown';
import { messageText } from '../core/presentation';
import { useTheme } from '../theme';
import { t } from '../i18n';
import type { useReplyOptions } from './use-reply-options';

const noAction = () => {};
export function ReplyOptions({ options }: { options: ReturnType<typeof useReplyOptions> }) {
  const { colors } = useTheme();
  const { entry, selection, selecting } = options;
  return (
    <MessageActions
      entry={entry}
      mode="actions"
      pending={options.pending}
      error={options.error}
      onClose={options.close}
      onReact={noAction}
      onReply={noAction}
      onThread={noAction}
      preview={
        entry ? (
          <View style={{ padding: 12, borderRadius: 16, backgroundColor: colors.surface }}>
            <Text
              style={{ color: colors.accent, fontSize: 15, fontWeight: '600', marginBottom: 6 }}
            >
              {entry.author.name}
            </Text>
            {selection && !selecting ? (
              <Text selectable style={{ color: colors.text, fontSize: 17 }}>
                {selection.text}
              </Text>
            ) : (
              <MessageMarkdown
                text={messageText(entry)}
                revision={entry.text_revision ?? entry.id}
                color={colors.text}
                fontSize={17}
                maxWidth={312}
                expandedForReveal
                selectable={selecting}
                onQuoteSelection={selecting ? options.review : undefined}
              />
            )}
          </View>
        ) : undefined
      }
      options={[
        {
          label: t(selection ? 'replyOptions.change' : 'replyOptions.select'),
          image: 'text.cursor',
          run: options.select,
          keepOpen: true,
          disabled: selecting || !entry?.text_revision || !('Message' in entry.kind),
        },
        { label: t('replyOptions.apply'), image: 'checkmark', run: options.apply, keepOpen: true },
        ...(options.canMove
          ? [
              {
                label: t('chat.replyInAnotherChat'),
                image: 'arrowshape.turn.up.right' as const,
                run: options.move,
                keepOpen: true,
              },
            ]
          : []),
        ...(selection
          ? [
              {
                label: t('replyOptions.remove'),
                image: 'text.quote' as const,
                run: options.remove,
                keepOpen: true,
              },
            ]
          : []),
        {
          label: t('replyOptions.cancel'),
          image: 'xmark',
          run: options.cancelReply,
          destructive: true,
          keepOpen: true,
        },
      ]}
    />
  );
}
