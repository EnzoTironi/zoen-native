import { FlashList } from '@shopify/flash-list';
import { Pressable, Text, View } from 'react-native';
import { NativeButton, Loading, EmptyState } from '../components/primitives';
import type { TimelineEntry } from '../core/types';
import { usePinnedText } from './preview';
import { useTheme } from '../theme';
import { t } from '../i18n';
import { useMessagePins } from './use-message-pins';
import { usePinnedNavigation } from './use-pinned-navigation';
export function PinnedList({ space, owner }: { space: string; owner: string }) {
  const { colors } = useTheme();
  const { session, state } = useMessagePins(space, null, owner || 'invalid-owner');
  const navigation = usePinnedNavigation(session, state.generation, space);
  if (!state.entries.length)
    return state.loading ? (
      <Loading />
    ) : (
      <EmptyState title={t(state.error ? 'messagePins.failed' : 'messagePins.empty')}>
        {state.error ? (
          <NativeButton
            label={t('chat.tryAgain2')}
            onPress={() => {
              void session.refresh();
            }}
          />
        ) : null}
      </EmptyState>
    );
  return (
    <FlashList
      data={state.entries}
      keyExtractor={(entry) => entry.id}
      renderItem={({ item }) => (
        <PinnedRow
          entry={item}
          space={space}
          pending={navigation.pending}
          error={navigation.error}
          open={navigation.open}
        />
      )}
      ListFooterComponent={
        <View style={{ padding: 16, gap: 12 }}>
          {state.error ? (
            <Text accessibilityRole="alert" style={{ color: colors.danger }}>
              {t('messagePins.failed')}
            </Text>
          ) : null}
          {state.next ? (
            <NativeButton
              label={t('messagePins.more')}
              disabled={state.loading}
              onPress={() => {
                void session.more(state.generation);
              }}
            />
          ) : null}
        </View>
      }
    />
  );
}

function PinnedRow({
  entry,
  space,
  pending,
  error,
  open,
}: {
  entry: TimelineEntry;
  space: string;
  pending: boolean;
  error: string | null;
  open(id: string): Promise<boolean>;
}) {
  const { colors } = useTheme();
  const text = usePinnedText(space, entry);
  return (
    <Pressable
      disabled={pending}
      accessibilityRole="button"
      accessibilityLabel={t('messagePins.open', { text: text })}
      onPress={() => {
        void open(entry.id);
      }}
      style={{ padding: 16, gap: 6, borderBottomWidth: 0.5, borderColor: colors.line }}
    >
      <Text style={{ color: colors.accent, fontWeight: '600', fontSize: 15 }}>
        {entry.author.name}
      </Text>
      <Text numberOfLines={4} style={{ color: colors.text, fontSize: 17 }}>
        {text}
      </Text>
      {error === entry.id ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {t('messagePins.unavailable')}
        </Text>
      ) : null}
    </Pressable>
  );
}
