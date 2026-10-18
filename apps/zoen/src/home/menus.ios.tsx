import { Host } from '@expo/ui';
import { Button, Menu } from '@expo/ui/swift-ui';
import { buttonStyle, frame, labelStyle } from '@expo/ui/swift-ui/modifiers';
import { router } from 'expo-router';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { HomeActionsProps, HomeCardMenuProps } from './menus.types';

export function HomeActions({ canOrganize = false }: HomeActionsProps) {
  const { scheme, colors } = useTheme();
  return (
    <Host testID="home-actions-host" matchContents colorScheme={scheme} seedColor={colors.accent}>
      <Menu
        label={t('home.add')}
        systemImage="plus"
        modifiers={[
          buttonStyle('glass'),
          labelStyle('iconOnly'),
          frame({ minWidth: 44, minHeight: 44 }),
        ]}
      >
        {canOrganize ? (
          <Button
            label={t('home.organize')}
            systemImage="arrow.up.arrow.down"
            onPress={() => router.push('/home-order')}
          />
        ) : null}
        <Button
          label={t('home.addMiniapp')}
          systemImage="square.grid.2x2"
          onPress={() => router.push('/miniapps')}
        />
        <Button
          label={t('home.newChat')}
          systemImage="square.and.pencil"
          onPress={() => router.push('/new-chat')}
        />
        <Button
          label={t('home.activity')}
          systemImage="square.stack"
          onPress={() => router.push('/activity')}
        />
        <Button
          label={t('home.search')}
          systemImage="magnifyingglass"
          onPress={() => router.push('/search')}
        />
      </Menu>
    </Host>
  );
}

export function HomeCardMenu({
  name,
  pinned,
  onToggle,
  onMoveEarlier,
  onMoveLater,
  onHistory,
  testID,
  conversationPinned,
  onToggleConversation,
  onOrganizeConversation,
}: HomeCardMenuProps) {
  const { scheme } = useTheme();
  return (
    <Host testID={testID ? `${testID}-host` : undefined} matchContents colorScheme={scheme}>
      <Menu
        label={t('home.options', { name })}
        systemImage="ellipsis"
        modifiers={[
          buttonStyle('glass'),
          labelStyle('iconOnly'),
          frame({ minWidth: 44, minHeight: 44 }),
        ]}
      >
        <Button
          label={t(pinned ? 'home.unpin' : 'home.pin')}
          systemImage={pinned ? 'pin.slash' : 'pin'}
          onPress={onToggle}
        />
        {onToggleConversation ? (
          <Button
            label={t(conversationPinned ? 'conversationCards.unpin' : 'conversationCards.pin')}
            systemImage={conversationPinned ? 'pin.slash' : 'pin'}
            onPress={onToggleConversation}
          />
        ) : null}
        {onOrganizeConversation ? (
          <Button
            label={t('conversationCards.manage')}
            systemImage="arrow.up.arrow.down"
            onPress={onOrganizeConversation}
          />
        ) : null}
        {onMoveEarlier ? (
          <Button label={t('home.moveEarlier')} systemImage="arrow.up" onPress={onMoveEarlier} />
        ) : null}
        {onMoveLater ? (
          <Button label={t('home.moveLater')} systemImage="arrow.down" onPress={onMoveLater} />
        ) : null}
        {onHistory ? (
          <Button
            label={t('miniapps.history')}
            systemImage="clock.arrow.circlepath"
            onPress={onHistory}
          />
        ) : null}
      </Menu>
    </Host>
  );
}
