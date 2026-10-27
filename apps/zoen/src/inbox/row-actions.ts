import type { ComponentProps } from 'react';
import type { Icon } from '../components/primitives';
import type { SpaceSummary } from '../core/generated/SpaceSummary';
import { t } from '../i18n/index.ts';

export type InboxAction = 'archive' | 'pin' | 'mute' | 'read';
export type RowAction = {
  id: InboxAction;
  label: string;
  icon: ComponentProps<typeof Icon>['name'];
  color: string;
};
export function rowActions(chat: SpaceSummary): RowAction[] {
  return [
    {
      id: 'read',
      label: t(chat.unread > 0 || chat.manual_unread ? 'inboxActions.read' : 'inboxActions.unread'),
      icon: {
        ios: chat.unread > 0 || chat.manual_unread ? 'envelope.open' : 'envelope.badge',
        android: chat.unread > 0 || chat.manual_unread ? 'drafts' : 'mark_email_unread',
      },
      color: chat.unread > 0 || chat.manual_unread ? '#8E8E93' : '#1993FA',
    },
    {
      id: 'pin',
      label: t(chat.pinned ? 'inboxActions.unpin' : 'inboxActions.pin'),
      icon: { ios: chat.pinned ? 'pin.slash' : 'pin', android: 'push_pin' },
      color: '#1993FA',
    },
    {
      id: 'mute',
      label: t(chat.muted ? 'inboxActions.unmute' : 'inboxActions.mute'),
      icon: {
        ios: chat.muted ? 'bell' : 'bell.slash',
        android: chat.muted ? 'notifications' : 'notifications_off',
      },
      color: '#FF9500',
    },
    {
      id: 'archive',
      label: t(chat.archived ? 'inboxActions.unarchive' : 'inboxActions.archive'),
      icon: { ios: chat.archived ? 'tray.and.arrow.up' : 'archivebox', android: 'archive' },
      color: '#A9A9AD',
    },
  ];
}
