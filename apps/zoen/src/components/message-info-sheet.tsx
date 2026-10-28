import { ScrollView, Text, View } from 'react-native';
import type { MessageInfo } from '../core/generated/MessageInfo';
import { messageInfoDate, messageReadDate } from '../core/message-info-presentation';
import { t, formatLocale } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { ProfileAvatar } from '../profile/profile-avatar';
import { NativeButton, Loading } from './primitives';
import { messageDeliveryLabel } from './message-receipt';

export function MessageInfoSheet({
  value,
  loading,
  error,
  retry,
}: {
  value: MessageInfo | null;
  loading: boolean;
  error: boolean;
  retry(): void;
}) {
  const { colors, presentation: p } = useTheme();
  const date = (at: string | null) =>
    messageInfoDate(at, formatLocale) ?? t('messageInfo.timeUnavailable');
  if (!value)
    return loading ? (
      <Loading />
    ) : (
      <View style={{ padding: 20, gap: 16 }}>
        <Text accessibilityRole="alert" style={{ color: colors.secondary }}>
          {t(error ? 'messageInfo.loadFailed' : 'messageInfo.unavailable')}
        </Text>
        {error ? <NativeButton label={t('chat.tryAgain2')} onPress={retry} /> : null}
      </View>
    );
  const row = (label: string, detail: string) => (
    <View style={{ paddingVertical: 12, gap: 4 }}>
      <Text style={{ color: colors.secondary, fontSize: 14 }}>{label}</Text>
      <Text selectable style={{ color: colors.text, fontSize: 17 }}>
        {detail}
      </Text>
    </View>
  );
  return (
    <ScrollView testID="message-info-sheet" contentContainerStyle={{ padding: 20, gap: 16 }}>
      <View style={{ flexDirection: 'row', gap: 12, alignItems: 'center' }}>
        <ProfileAvatar person={value.author} name={value.author.name} size={44} />
        <View style={{ flex: 1, gap: 4 }}>
          <Text style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}>
            {value.author.name}
          </Text>
          <Text style={{ color: colors.secondary }}>{messageDeliveryLabel(value.delivery)}</Text>
        </View>
      </View>
      <View style={{ backgroundColor: p.list.block, paddingHorizontal: 16, borderRadius: 14 }}>
        {row(t('messageInfo.sent'), date(value.sent_at_ms))}
        {value.edited ? row(t('messageInfo.edited'), date(value.edited_at_ms)) : null}
      </View>
      <Text style={{ color: colors.secondary, fontSize: 13 }}>{t('messageInfo.authorClock')}</Text>
      {value.read_details === 'available' && value.participants.length ? (
        <View style={{ gap: 12 }}>
          <Text
            accessibilityRole="header"
            style={{ color: colors.text, fontSize: 17, fontWeight: '600' }}
          >
            {t('messageInfo.readReceipts')}
          </Text>
          {value.participants.map((participant) => (
            <View
              key={participant.person.id}
              style={{ flexDirection: 'row', gap: 12, alignItems: 'center', minHeight: 56 }}
            >
              <ProfileAvatar person={participant.person} name={participant.person.name} size={40} />
              <View style={{ flex: 1, gap: 3 }}>
                <Text style={{ color: colors.text, fontSize: 17 }}>{participant.person.name}</Text>
                <Text style={{ color: colors.secondary }}>
                  {participant.read
                    ? t('messageInfo.read')
                    : participant.delivered
                      ? t('messageInfo.delivered')
                      : t('messageInfo.pending')}
                  {participant.read ? ` · ${date(messageReadDate(participant))}` : ''}
                </Text>
              </View>
            </View>
          ))}
          <Text style={{ color: colors.secondary, fontSize: 13 }}>
            {t('messageInfo.peerClock')}
          </Text>
        </View>
      ) : value.read_details === 'not_small_conversation' ? (
        <Text style={{ color: colors.secondary }}>{t('messageInfo.smallConversationOnly')}</Text>
      ) : null}
    </ScrollView>
  );
}
