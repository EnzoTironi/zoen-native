import { messageCanSelect } from '../core/message-presentation.ts';
import type { TimelineEntry } from '../core/types';
import type { MessageContextAction } from '../../modules/zoen-core/src/ZoenMessageContext.types';
import type { MessageActionRow, MessageActionsProps } from './message-actions.types';
import { t } from '../i18n/index.ts';

export type MessageMenuAction = Omit<MessageActionRow, 'run'> & {
  id: MessageContextAction;
  run(entry: TimelineEntry): void;
};

/** All message menu presentations consume the same action list. Core remains the authority. */
export function messageMenuActions(props: MessageActionsProps | null): MessageMenuAction[] {
  const entry = props?.entry;
  if (!props || !entry || !('Message' in entry.kind)) return [];
  const live = !entry.deleted;
  const rows: MessageMenuAction[] = [];
  function add(
    id: MessageContextAction,
    label: string,
    image: MessageMenuAction['image'],
    run: ((entry: TimelineEntry) => void) | undefined,
    eligible = true,
    destructive = false,
  ) {
    if (run && eligible) rows.push({ id, label, image, run, destructive });
  }
  add('reply', t('chat.reply'), 'arrowshape.turn.up.left', props.onReply, live);
  add(
    'thread',
    t('chat.openReplies'),
    'bubble.left.and.bubble.right',
    props.onThread,
    live || entry.thread_replies > 0,
  );
  add('info', t('messageInfo.title'), 'info.circle', props.onInfo, live);
  add('forward', t('chat.forward'), 'arrowshape.turn.up.right', props.onForward, live);
  add(
    'selectMessages',
    t('chat.selectMessages'),
    'checkmark.circle',
    props.onSelect,
    messageCanSelect(entry),
  );
  add(
    'select',
    t('chat.selectText'),
    'text.cursor',
    props.onSelectText,
    live && Boolean(props.nativeSource?.textSelection),
  );
  add('copy', t('chat.copyText'), 'doc.on.doc', props.onCopy, live);
  add('copyLink', t('chat.copyMessageLink'), 'link', props.onCopyLink, live);
  add('share', t('media.share'), 'square.and.arrow.up', props.onShare, live);
  add(
    'recover',
    t('media.prepareNewSend'),
    'arrow.clockwise',
    props.onRecover,
    live && entry.author.is_me && entry.delivery === 'Failed',
  );
  add('edit', t('chat.edit'), 'pencil', props.onEdit, live && entry.author.is_me);
  add('delete', t('chat.delete'), 'trash', props.onDelete, messageCanSelect(entry), true);
  return rows;
}
