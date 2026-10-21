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
      ['forward', t('chat.forward'), !entry.deleted ? props.onForward : undefined],
      ['copy', t('chat.copyText'), props.onCopy],
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
    function selected(value: MessageContextSelection) {
      const props = latest.current,
        entry = props.entry;
      if (!entry || !current() || !request?.current()) return;
      // Consume the presentation before invoking canonical code or navigation.
      if (value.startsWith('reaction:')) {
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
          forward: props.onForward,
          copy: props.onCopy,
          share: props.onShare,
          edit: props.onEdit,
          recover: props.onRecover,
          delete: props.onDelete,
        }[value as MessageContextAction];
        run?.(entry);
      }
    }
    const owner = new MessageContextPresentation({
      request,
      current,
      id: randomUUID,
      selected,
      dismissed: () => {
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
      live = false;
      void owner.retire().catch(() => {});
    };
  }, [request, messageId]);
  return failed || props.error ? fallback({ ...props, nativeSource: undefined }) : null;
}
