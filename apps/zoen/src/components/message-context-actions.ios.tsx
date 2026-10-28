import { selectedMessageQuote } from '../core/selected-message-quote';
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { randomUUID } from 'expo-crypto';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { MessageActionsProps } from './message-actions.types';
import type {
  MessageContextAction,
  MessageContextEmoji,
  MessageContextSelection,
} from '../../modules/zoen-core/src/ZoenMessageContext.types';
import { MessageContextPresentation } from './message-context-presentation';

const emoji: readonly MessageContextEmoji[] = ['👍', '❤️', '😂', '😮', '😢', '🙏'];
export function ExtractedMessageActions({
  props,
  fallback,
}: {
  props: MessageActionsProps;
  fallback(props: MessageActionsProps): ReactNode;
}) {
  const { presentation, reducedMotion } = useTheme();
  const latest = useRef(props);
  const appearance = useRef({ context: presentation.context, reducedMotion });
  useLayoutEffect(() => {
    latest.current = props;
    appearance.current = { context: presentation.context, reducedMotion };
  }, [props, presentation.context, reducedMotion]);
  const [failed, setFailed] = useState(false);
  const request = props.nativeSource;
  const messageId = props.entry?.id;
  useEffect(() => {
    const props = latest.current;
    const entry = props.entry;
    if (
      !request ||
      !entry ||
      entry.id !== messageId ||
      props.nativeSource !== request ||
      !request.current()
    )
      return;
    const visual = appearance.current;
    let live = true;
    const initial = entry.id;
    const callbacks: ReadonlyArray<
      readonly [MessageContextAction, string, MessageActionsProps['onReply'] | undefined]
    > = [
      ['reply', t('chat.reply'), props.onReply],
      ['thread', t('chat.openReplies'), props.onThread],
      ['info', t('messageInfo.title'), props.onInfo],
      ['forward', t('chat.forward'), !entry.deleted ? props.onForward : undefined],
      ['selectMessages', t('chat.selectMessages'), !entry.deleted ? props.onSelect : undefined],
      ['select', t('chat.selectText'), request.textSelection ? props.onSelectText : undefined],
      ['copy', t('chat.copyText'), props.onCopy],
      ['copyLink', t('chat.copyMessageLink'), props.onCopyLink],
      ['share', t('media.share'), props.onShare],
      [
        'recover',
        t('media.prepareNewSend'),
        entry.author.is_me && !entry.deleted && entry.delivery === 'Failed'
          ? props.onRecover
          : undefined,
      ],
      ['edit', t('chat.edit'), entry.author.is_me && !entry.deleted ? props.onEdit : undefined],
      [
        'delete',
        t('chat.deleteForEveryone'),
        entry.author.is_me && !entry.deleted ? props.onDelete : undefined,
      ],
    ];
    const current = () =>
      live &&
      latest.current.entry?.id === initial &&
      latest.current.nativeSource === request &&
      !latest.current.pending &&
      latest.current.isCurrent?.() !== false;
    let cancelQuote = () => {};
    function selected(value: MessageContextSelection) {
      const props = latest.current,
        entry = props.entry;
      if (!entry || !current() || !request?.current()) return;
      // Consume the presentation before invoking canonical code or navigation.
      if (value.startsWith('select:')) {
        try {
          const result = JSON.parse(value.slice(7));
          const quote = selectedMessageQuote(entry, result.text, JSON.stringify(result.spans));
          if (quote && quote.source_id === result.source_id && quote.revision === result.revision)
            props.onQuote?.(entry, quote);
        } catch {
          /* Malformed native results never become draft state. */
        }
        cancelQuote();
        props.onClose();
      } else if (value.startsWith('reaction:')) {
        const selected = value.slice('reaction:'.length);
        props.onReact(
          entry.reactions.some((reaction) => reaction.mine && reaction.emoji === selected)
            ? null
            : selected,
        );
      } else if (value === 'more') {
        props.onClose();
        props.onMore?.();
      } else {
        props.onClose();
        const run = {
          reply: props.onReply,
          thread: props.onThread,
          info: props.onInfo,
          forward: props.onForward,
          copy: props.onCopy,
          copyLink: props.onCopyLink,
          select: props.onSelectText,
          selectMessages: props.onSelect,
          share: props.onShare,
          edit: props.onEdit,
          recover: props.onRecover,
          delete: props.onDelete,
        }[value as MessageContextAction];
        run?.(entry);
      }
    }
    const detachSelection = request.textSelection?.bind({
      started: () => {
        if (!current()) return;
        cancelQuote();
        const cancel = latest.current.onSelectText?.(entry);
        if (typeof cancel === 'function') cancelQuote = cancel;
      },
      review: async (text, ranges) => {
        if (!current() || !request.current() || !latest.current.onQuoteReview)
          throw new Error('Selection retired');
        const quote = await latest.current.onQuoteReview(entry, text, ranges);
        if (!current() || !request.current()) throw new Error('Selection retired');
        return JSON.stringify(quote);
      },
    });
    const owner = new MessageContextPresentation({
      request,
      current,
      id: randomUUID,
      selected,
      dismissed: () => {
        cancelQuote();
        if (current() && request.current()) latest.current.onClose();
      },
    });
    void owner
      .open({
        actions:
          props.mode === 'actions'
            ? callbacks.filter(([, , run]) => run).map(([id, label]) => ({ id, label }))
            : [],
        reactions: emoji.map((value) => ({
          emoji: value,
          label: t(
            entry.reactions.some((reaction) => reaction.mine && reaction.emoji === value)
              ? 'chat.removeReaction'
              : 'chat.reactWithEmoji',
            { emoji: value },
          ),
          selected: entry.reactions.some((reaction) => reaction.mine && reaction.emoji === value),
        })),
        moreLabel: props.onMore ? t('chat.moreReactions') : undefined,
        theme: visual.context,
        reducedMotion: visual.reducedMotion,
        incoming: !entry.author.is_me,
      })
      .catch(async () => {
        // A refused source/geometry may use the existing accessible safe menu,
        // but only after exact native removal succeeds. Never overlay two owners.
        try {
          await owner.close();
          if (owner.current() && live) setFailed(true);
        } catch {
          /* root retirement remains responsible for native drain */
        }
      });
    return () => {
      detachSelection?.();
      cancelQuote();
      live = false;
      void owner.retire().catch(() => {});
    };
  }, [request, messageId]);
  return failed || props.error ? fallback({ ...props, nativeSource: undefined }) : null;
}
