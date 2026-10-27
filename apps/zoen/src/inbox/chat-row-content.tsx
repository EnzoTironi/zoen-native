import { Text, View, StyleSheet } from 'react-native';
import { formatLocale } from '../i18n';
import { ConversationAvatar } from '../saved-messages/conversation-avatar';
import { conversationTitle } from '../saved-messages/presentation';
import { MessageReceipt } from '../components/message-receipt';
import { Icon } from '../components/primitives';
import { useTheme } from '../theme';
import type { SpaceSummary } from '../core/generated/SpaceSummary';

/** Existing Telegram inbox presentation; canonical drafts/previews stay in Core. */
export function ChatRowContent({ chat }: { chat: SpaceSummary }) {
  const { presentation: p } = useTheme();
  return (
    <>
      <ConversationAvatar chat={chat} size={60} />
      <View style={{ flex: 1, gap: 3 }}>
        <View
          style={{
            flexDirection: 'row',
            justifyContent: 'space-between',
            alignItems: 'baseline',
            gap: 8,
          }}
        >
          <Text
            numberOfLines={1}
            style={{
              flex: 1,
              fontSize: 16,
              fontWeight: '600',
              color: p.chatList.title,
            }}
          >
            {conversationTitle(chat)}
          </Text>
          {chat.last_author?.is_me && chat.last_delivery ? (
            <MessageReceipt
              delivery={chat.last_delivery}
              ownerKey={`${chat.id}:${chat.last_at_ms}`}
              ink={p.chatList.date}
              background={p.chatList.background}
            />
          ) : null}
          <Text style={{ fontSize: 14, color: p.chatList.date }}>
            {Number(chat.last_at_ms)
              ? new Date(Number(chat.last_at_ms)).toLocaleTimeString(formatLocale, {
                  hour: '2-digit',
                  minute: '2-digit',
                })
              : ''}
          </Text>
        </View>
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
          <Text
            numberOfLines={2}
            style={{ flex: 1, lineHeight: 19, fontSize: 15, color: p.chatList.message }}
          >
            {chat.last_preview}
          </Text>
          {chat.unread ? (
            <View
              style={{
                backgroundColor: chat.muted ? p.chatList.mutedUnread : p.chatList.unread,
                minWidth: 20,
                height: 20,
                borderRadius: 10,
                paddingHorizontal: 6,
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <Text
                style={{
                  color: chat.muted ? '#FFFFFF' : p.chatList.unreadText,
                  fontSize: 12,
                  fontWeight: '600',
                  fontVariant: ['tabular-nums'],
                }}
              >
                {chat.unread > 99 ? '99+' : chat.unread}
              </Text>
            </View>
          ) : chat.pinned ? (
            <Icon
              name={{ ios: 'pin.fill', android: 'push_pin' }}
              size={14}
              color={p.chatList.pinnedIcon}
            />
          ) : chat.muted ? (
            <Icon
              name={{ ios: 'bell.slash.fill', android: 'notifications_off' }}
              size={14}
              color={p.chatList.pinnedIcon}
            />
          ) : null}
        </View>
      </View>
      <View
        pointerEvents="none"
        style={{
          position: 'absolute',
          left: 86,
          right: 0,
          bottom: 0,
          height: StyleSheet.hairlineWidth,
          backgroundColor: p.separator,
        }}
      />
    </>
  );
}
