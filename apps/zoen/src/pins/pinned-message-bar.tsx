import { useState } from 'react';
import { Pressable, Text, View } from 'react-native';
import { router } from 'expo-router';
import { NavigationGlass } from '../components/navigation-glass';
import { IconButton } from '../components/primitives';
import { usePinnedText } from './preview';
import { useTheme } from '../theme';
import { t } from '../i18n';
import { useMessagePins } from './use-message-pins';
import { usePinnedNavigation } from './use-pinned-navigation';
export function PinnedMessageBar({
  space,
  onReveal,
}: {
  space: string;
  onReveal(message: string): Promise<void>;
}) {
  const { colors } = useTheme();
  const { session, state, owner } = useMessagePins(space);
  const [position, setPosition] = useState({ revision: '', index: 0 });
  const index =
    position.revision === state.revision ? position.index % Math.max(1, state.entries.length) : 0;
  const entry = state.entries[index];
  const text = usePinnedText(space, entry);
  const current = () => session.current(state.generation);
  const list = () => {
    if (current())
      router.push({ pathname: '/pinned/[space]', params: { space, owner: owner ?? '' } });
  };
  const navigation = usePinnedNavigation(session, state.generation, space, onReveal);
  const failed = navigation.error === entry?.id;
  if (!entry)
    return state.error ? (
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={t('chat.tryAgain2')}
        onPress={() => {
          void session.refresh();
        }}
        style={{ padding: 12 }}
      >
        <Text style={{ color: colors.danger }}>{t('messagePins.failed')}</Text>
      </Pressable>
    ) : null;
  return (
    <NavigationGlass radius={0}>
      <View
        style={{
          flexDirection: 'row',
          alignItems: 'center',
          paddingHorizontal: 12,
          paddingVertical: 6,
        }}
      >
        <Pressable
          testID="pinned-message-open"
          disabled={navigation.pending}
          accessibilityRole="button"
          accessibilityLabel={t('messagePins.open', { text })}
          onLongPress={list}
          onPress={() => {
            if (!current()) return;
            void navigation.open(entry.id).then(async (accepted) => {
              if (!accepted || !current()) return;
              if (index + 1 === state.entries.length && state.next) {
                await session.more(state.generation);
                if (!current()) return;
              }
              setPosition({ revision: state.revision, index: index + 1 });
            });
          }}
          style={{
            flex: 1,
            borderLeftWidth: 3,
            borderColor: colors.accent,
            paddingLeft: 10,
            gap: 2,
            minHeight: 44,
            justifyContent: 'center',
          }}
        >
          <Text style={{ color: colors.accent, fontSize: 14, fontWeight: '600' }}>
            {t('messagePins.label')}
          </Text>
          <Text
            numberOfLines={1}
            style={{ color: failed ? colors.danger : colors.text, fontSize: 15 }}
          >
            {failed ? t('messagePins.unavailable') : text}
          </Text>
        </Pressable>
        {state.total > 1 ? (
          <Text style={{ color: colors.secondary, fontSize: 12, marginLeft: 6 }}>
            {index + 1}/{state.total}
          </Text>
        ) : null}
        <IconButton
          name={{ ios: 'list.bullet', android: 'list', web: 'list' }}
          label={t('messagePins.list')}
          onPress={list}
        />
      </View>
    </NavigationGlass>
  );
}
