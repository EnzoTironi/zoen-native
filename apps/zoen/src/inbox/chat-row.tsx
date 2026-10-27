// Telegram-iOS f1dd7a2 ChatListItem: leading Read/Pin, trailing Mute/Archive.
// FlashList owns virtualization; SwiftUI .swipeActions requires its own native container.
import { useEffect, useRef } from 'react';
import { Pressable } from 'react-native';
import Swipeable, { type SwipeableMethods } from 'react-native-gesture-handler/ReanimatedSwipeable';
import { ReduceMotion } from 'react-native-reanimated';
import { selectionAsync } from 'expo-haptics';
import { MenuView } from '@expo/ui/community/menu';
import type { SpaceSummary } from '../core/generated/SpaceSummary';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { SwipeActionPanel } from './swipe-action-panel';
import { ChatRowContent } from './chat-row-content';
import { rowActions, type InboxAction } from './row-actions';

export function InboxChatRow({
  chat,
  enabled,
  pending,
  onOpen,
  onAction,
}: {
  chat: SpaceSummary;
  enabled: boolean;
  pending: boolean;
  onOpen(): void;
  onAction(action: InboxAction): void;
}) {
  const { presentation: p, reducedMotion } = useTheme();
  const swipe = useRef<SwipeableMethods | null>(null);
  const actions = enabled ? rowActions(chat) : [];
  useEffect(() => {
    if (!enabled || pending) swipe.current?.reset();
  }, [enabled, pending]);
  const run = (action: InboxAction) => {
    swipe.current?.close();
    if (enabled && !pending && actions.some((item) => item.id === action)) onAction(action);
  };
  const content = (
    <Pressable
      testID={`inbox-chat-${chat.id}`}
      accessibilityRole="button"
      accessibilityLabel={
        chat.unread
          ? t('chat.conversationUnreadLabel', {
              title: chat.title,
              count: chat.unread,
              preview: chat.last_preview,
            })
          : t('chat.conversationLabel', { title: chat.title, preview: chat.last_preview })
      }
      accessibilityState={{ busy: pending }}
      accessibilityActions={actions.map((action) => ({ name: action.id, label: action.label }))}
      onAccessibilityAction={({ nativeEvent }) => run(nativeEvent.actionName as InboxAction)}
      onPress={onOpen}
      style={({ pressed }) => ({
        flexDirection: 'row',
        gap: 10,
        paddingHorizontal: 16,
        paddingVertical: 8,
        minHeight: 76,
        alignItems: 'center',
        backgroundColor: pressed
          ? p.chatList.highlighted
          : chat.pinned
            ? `${p.chatList.pinned}CC`
            : 'transparent',
      })}
    >
      <ChatRowContent chat={chat} />
    </Pressable>
  );
  if (!enabled) return content;
  const leading = chat.archived
    ? []
    : actions.filter((action) => action.id === 'read' || action.id === 'pin');
  const trailing = actions.filter(
    (action) => action.id === 'archive' || action.id === (chat.archived ? 'pin' : 'mute'),
  );
  return (
    <Swipeable
      ref={swipe}
      testID={`inbox-swipe-${chat.id}`}
      enabled={!pending}
      overshootLeft={false}
      overshootRight={false}
      animationOptions={{ reduceMotion: reducedMotion ? ReduceMotion.Always : ReduceMotion.System }}
      renderLeftActions={(_progress, translation) =>
        leading.length ? (
          <SwipeActionPanel
            actions={leading}
            translation={translation}
            edge="leading"
            id={chat.id}
            pending={pending}
            onAction={run}
          />
        ) : null
      }
      renderRightActions={(_progress, translation) => (
        <SwipeActionPanel
          actions={trailing}
          translation={translation}
          edge="trailing"
          id={chat.id}
          pending={pending}
          onAction={run}
        />
      )}
      onSwipeableWillOpen={() => {
        void selectionAsync().catch(() => {});
      }}
    >
      <MenuView
        testID={`inbox-menu-${chat.id}`}
        shouldOpenOnLongPress
        style={{ width: '100%' }}
        actions={actions.map((action) => ({
          id: action.id,
          title: action.label,
          image: typeof action.icon === 'object' ? action.icon.ios : undefined,
          attributes: { disabled: pending },
        }))}
        onPressAction={({ nativeEvent }) => run(nativeEvent.event as InboxAction)}
      >
        {content}
      </MenuView>
    </Swipeable>
  );
}
