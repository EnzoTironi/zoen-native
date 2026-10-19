import { t } from '../i18n/index.ts';
import type { HomeCardMenuProps } from './menus.types';

export type HomeCardActionId =
  | 'home-pin'
  | 'conversation-pin'
  | 'conversation-organize'
  | 'move-earlier'
  | 'move-later'
  | 'history';
export type HomeCardAction = {
  id: HomeCardActionId;
  label: string;
  onPress: () => void;
};

/** Each supplied action remains independently available, in the native menu order. */
export function homeCardActions({
  pinned,
  conversationPinned,
  onToggle,
  onToggleConversation,
  onOrganizeConversation,
  onMoveEarlier,
  onMoveLater,
  onHistory,
}: HomeCardMenuProps): HomeCardAction[] {
  const actions: HomeCardAction[] = [];
  if (onToggle)
    actions.push({
      id: 'home-pin',
      label: t(pinned ? 'home.unpin' : 'home.pin'),
      onPress: onToggle,
    });
  if (onToggleConversation)
    actions.push({
      id: 'conversation-pin',
      label: t(conversationPinned ? 'conversationCards.unpin' : 'conversationCards.pin'),
      onPress: onToggleConversation,
    });
  if (onOrganizeConversation)
    actions.push({
      id: 'conversation-organize',
      label: t('conversationCards.manage'),
      onPress: onOrganizeConversation,
    });
  if (onMoveEarlier)
    actions.push({ id: 'move-earlier', label: t('home.moveEarlier'), onPress: onMoveEarlier });
  if (onMoveLater)
    actions.push({ id: 'move-later', label: t('home.moveLater'), onPress: onMoveLater });
  if (onHistory) actions.push({ id: 'history', label: t('miniapps.history'), onPress: onHistory });
  return actions;
}
