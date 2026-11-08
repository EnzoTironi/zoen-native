import { ScrollView, Text, View } from 'react-native';
import { NativeButton, Loading } from '../components/primitives';
import { ForwardMessagePreview } from '../components/forward-message-preview';
import { useTheme } from '../theme';
import { t } from '../i18n';
import { useMessagePins } from './use-message-pins';
export function PinReview({
  space,
  message,
  owner,
}: {
  space: string;
  message: string;
  owner: string;
}) {
  const { session, state } = useMessagePins(space, message, owner || 'invalid-owner');
  const { colors } = useTheme();
  const { review, generation } = state;
  if (!review)
    return state.loading ? (
      <Loading />
    ) : (
      <View style={{ padding: 20, gap: 16 }}>
        <Text accessibilityRole="alert" style={{ color: colors.secondary }}>
          {t(state.error ? 'messagePins.failed' : 'messagePins.unavailable')}
        </Text>
        {state.error ? (
          <NativeButton
            label={t('chat.tryAgain2')}
            onPress={() => {
              void session.refresh();
            }}
          />
        ) : null}
      </View>
    );
  return (
    <ScrollView contentContainerStyle={{ padding: 20, gap: 18 }}>
      <View style={{ borderLeftWidth: 3, borderColor: colors.accent, paddingLeft: 12, gap: 6 }}>
        <Text style={{ color: colors.accent, fontSize: 15, fontWeight: '600' }}>
          {review.entry.author.name}
        </Text>
        <ForwardMessagePreview
          spaceId={space}
          entry={review.entry}
          isCurrent={() => session.current(generation)}
        />
      </View>
      {state.actionError || state.error ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {t('messagePins.actionFailed')}
        </Text>
      ) : null}
      {review.pending ? (
        <Text accessibilityRole="status" style={{ color: colors.secondary }}>
          {t('messagePins.pending')}
        </Text>
      ) : null}
      {review.can_pin ? (
        <NativeButton
          testID="message-pin-toggle"
          label={t(review.pinned ? 'messagePins.unpin' : 'messagePins.pin')}
          disabled={state.pending || state.loading}
          onPress={() => {
            void session.toggle(generation);
          }}
        />
      ) : (
        <Text style={{ color: colors.secondary }}>{t('messagePins.cannotPin')}</Text>
      )}
    </ScrollView>
  );
}
