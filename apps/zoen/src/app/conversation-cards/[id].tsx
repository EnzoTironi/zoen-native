import { Stack, useLocalSearchParams, router } from 'expo-router';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { EmptyState, Loading, NativeButton } from '../../components/primitives';
import { SkyScene } from '../../components/sky-background';
import { useClient } from '../../core/provider';
import { ConversationOrderList } from '../../miniapps/conversation-order-list';
import { useConversationCards } from '../../miniapps/conversation-provider';
import { t } from '../../i18n';
import { useTheme } from '../../theme';

export default function ConversationCards() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const cards = useConversationCards(id);
  const { snapshot } = useClient();
  const chat = cards.available
    ? snapshot?.conversations.find((space) => space.id === id)
    : undefined;
  const headerHeight = useHeaderHeight();
  const insets = useSafeAreaInsets();
  const { colors } = useTheme();
  return (
    <SkyScene>
      <Stack.Screen options={{ title: t('conversationCards.organize') }} />
      <View style={{ flex: 1, paddingTop: headerHeight, paddingBottom: insets.bottom }}>
        <View style={{ padding: 24, gap: 8 }}>
          {chat ? (
            <Text style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}>
              {chat.title}
            </Text>
          ) : null}
          <Text style={{ color: colors.secondary, fontSize: 16, lineHeight: 23 }}>
            {t('conversationCards.organizeHint')}
          </Text>
        </View>
        {cards.error ? (
          <View style={{ paddingHorizontal: 24, paddingBottom: 16, gap: 12 }}>
            <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
              {cards.error}
            </Text>
            {cards.retryable ? (
              <NativeButton
                secondary
                label={t('conversationCards.tryAgain')}
                onPress={cards.retry}
              />
            ) : null}
          </View>
        ) : null}
        {!cards.available ? (
          <EmptyState
            title={t('conversationCards.unavailable')}
            detail={t('chat.thisChatIsNoLongerOnThisDevice')}
          />
        ) : cards.loading ? (
          <Loading />
        ) : cards.items.length ? (
          <ConversationOrderList
            key={`${cards.ownerKey}:${id}`}
            items={cards.items}
            pinnedIds={cards.pinnedIds}
            onReorder={cards.reorder}
            onSetPinned={cards.setPinned}
          />
        ) : !cards.error ? (
          <EmptyState
            title={t('conversationCards.empty')}
            detail={t('conversationCards.emptyDetail')}
          >
            {chat ? (
              <NativeButton
                label={t('home.addMiniapp')}
                onPress={() => router.push({ pathname: '/miniapps', params: { space: id } })}
              />
            ) : null}
          </EmptyState>
        ) : null}
      </View>
    </SkyScene>
  );
}
