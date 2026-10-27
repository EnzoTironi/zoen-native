import { t } from '../i18n/index.ts';
import { ProfileAvatar } from '../profile/profile-avatar';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
  useSyncExternalStore,
} from 'react';
import { Stack, router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { FlashList } from '@shopify/flash-list';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { Pressable, Text, TextInput, View } from 'react-native';
import { KeyboardAvoidingView, KeyboardController } from 'react-native-keyboard-controller';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { homeOwnerFor, homeSpacesFor } from '../home/access';
import { GlobalSearchActions } from '../core/global-search-actions';
import { searchScope, useSearchResults } from '../core/use-search-results';
import { useClient } from '../core/provider';
import type { UniversalHit } from '../core/generated/UniversalHit';
import { useTheme } from '../theme';
import { EmptyState, Loading, NativeButton } from '../components/primitives';

const plain = (text: string) => text.replace(/\[\[|\]\]/g, '');

export default function Search() {
  const params = useLocalSearchParams<{ space?: string; query?: string }>();
  const { store } = useClient();
  const scope = searchScope(store, params.space);
  const owner = JSON.stringify([
    store.sessionEpoch,
    homeOwnerFor(store.getSnapshot()),
    store.getSnapshot().snapshot?.account?.relay_url ?? null,
  ]);
  const [initialOwner] = useState(owner);
  return (
    <SearchBody
      key={JSON.stringify([owner, params.space, Boolean(scope), params.query])}
      space={params.space}
      initial={scope && owner === initialOwner ? params.query : undefined}
      scope={scope}
    />
  );
}

function SearchBody({
  space,
  initial,
  scope,
}: {
  space?: string;
  initial?: string;
  scope: string | null;
}) {
  const headerHeight = useHeaderHeight();
  const insets = useSafeAreaInsets();
  const { store } = useClient();
  const { colors } = useTheme();
  const { search, state: results } = useSearchResults(store, scope, space, initial);
  const query = results.query;
  const actions = useMemo(
    () =>
      new GlobalSearchActions(
        () =>
          scope !== null && searchScope(store, space) === scope && results === search.getSnapshot(),
        query,
      ),
    [scope, space, query, store, search, results],
  );
  const operation = useSyncExternalStore(
    actions.subscribe,
    actions.getSnapshot,
    actions.getSnapshot,
  );
  useLayoutEffect(() => () => actions.blur(), [actions]);
  useFocusEffect(
    useCallback(() => {
      actions.focus();
      const unsubscribe = store.subscribe(() => actions.refresh());
      return () => {
        unsubscribe();
        actions.blur();
      };
    }, [actions, store]),
  );
  const opening = operation.pending;
  useEffect(() => {
    if (!results.active || results.loading || results.completed || results.error || !query.trim())
      return;
    const timer = setTimeout(() => void search.search(), 200);
    return () => clearTimeout(timer);
  }, [search, query, results.active, results.loading, results.completed, results.error]);
  const hits = scope ? results.hits : [];
  const queryFits = search.fitsQuery(query.trim());
  const error = !queryFits ? t('chat.searchQueryTooLong') : (operation.error ?? results.error);
  function open(hit: UniversalHit) {
    void actions.run(
      async () => {
        if (
          hit.kind === 'message' &&
          hit.space_id &&
          !homeSpacesFor(store.getSnapshot()).has(hit.space_id)
        )
          throw new Error(t('chat.thisResultCouldNotBeOpened'));
        const destination =
          hit.kind === 'message' && hit.space_id
            ? await store.driver.request({
                op: 'message',
                space_id: hit.space_id,
                message_id: hit.ref_id,
              })
            : null;
        if (!actions.isCurrent()) return null;
        await KeyboardController.dismiss();
        return destination;
      },
      (message) => {
        if (hit.space_id && !homeSpacesFor(store.getSnapshot()).has(hit.space_id)) return;
        if (hit.kind === 'message' && hit.space_id && message) {
          if (message.id !== hit.ref_id || message.deleted)
            throw new Error(t('chat.thisResultCouldNotBeOpened'));
          if (message.in_thread)
            router.push({
              pathname: '/thread/[space]/[root]',
              params: { space: hit.space_id, root: message.in_thread },
            });
          else
            router.push({ pathname: '/chat/[id]', params: { id: hit.space_id, jump: hit.ref_id } });
        } else if (hit.kind === 'space')
          router.push({ pathname: '/chat/[id]', params: { id: hit.ref_id } });
        else if (hit.kind === 'person' || hit.kind === 'agent')
          router.push({ pathname: '/person/[id]', params: { id: hit.ref_id } });
        else router.push({ pathname: '/item/[id]', params: { id: hit.ref_id } });
      },
      t('chat.thisResultCouldNotBeOpened'),
    );
  }
  const empty = !scope ? (
    <EmptyState title={t('chat.chatUnavailable')} />
  ) : !query.trim() ? (
    <EmptyState
      title={t('chat.findWhatStayedInTheConversation')}
      detail={t('chat.searchUsesTheHistoryAvailableOnThisDevice')}
      icon={{ ios: 'magnifyingglass', android: 'search', web: 'search' }}
    />
  ) : !results.completed && !error ? (
    <Loading />
  ) : error ? null : (
    <EmptyState title={t('chat.noResults')} detail={t('chat.tryAnotherWordOrName')} />
  );
  return (
    <KeyboardAvoidingView
      behavior="padding"
      automaticOffset
      style={{ flex: 1, paddingTop: headerHeight, backgroundColor: 'transparent' }}
    >
      <Stack.Screen options={{ title: space ? t('chat.searchThisChat') : t('chat.search') }} />
      <TextInput
        testID="global-search-input"
        accessibilityLabel={t('chat.search')}
        autoFocus
        value={query}
        onChangeText={(value) => {
          if (search.isCurrent()) search.setQuery(value);
        }}
        autoCorrect={false}
        returnKeyType="search"
        placeholder={space ? t('chat.messagesInThisChat') : t('chat.peopleMessagesPages')}
        placeholderTextColor={colors.tertiary}
        style={{
          margin: 20,
          minHeight: 52,
          padding: 15,
          borderRadius: 18,
          backgroundColor: colors.muted,
          color: colors.text,
          fontSize: 17,
        }}
      />
      <FlashList
        testID="global-search-results"
        contentInsetAdjustmentBehavior="never"
        contentContainerStyle={{ flexGrow: 1, paddingBottom: insets.bottom + 16 }}
        data={hits}
        keyExtractor={(hit) => `${hit.kind}:${hit.space_id}:${hit.ref_id}`}
        keyboardShouldPersistTaps="handled"
        keyboardDismissMode={process.env.EXPO_OS === 'ios' ? 'interactive' : 'on-drag'}
        ListHeaderComponent={
          error ? (
            <View style={{ paddingHorizontal: 24, paddingBottom: 16, gap: 12 }}>
              <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
                {error}
              </Text>
              <NativeButton
                secondary
                label={t('chat.tryAgain2')}
                disabled={!queryFits}
                onPress={() => {
                  actions.clearError();
                  if (results === search.getSnapshot() && search.isCurrent()) void search.search();
                }}
              />
            </View>
          ) : null
        }
        ListEmptyComponent={empty}
        renderItem={({ item }) => (
          <Pressable
            accessibilityRole="button"
            disabled={opening}
            onPress={() => {
              void open(item);
            }}
            style={{
              paddingHorizontal: 24,
              paddingVertical: 17,
              flexDirection: 'row',
              gap: 13,
              borderBottomWidth: 1,
              borderColor: colors.line,
            }}
          >
            {item.persona ? (
              <ProfileAvatar
                person={item.persona}
                name={item.persona.name}
                color={item.persona.tint_hex}
                agent={item.persona.kind === 'Agent'}
                size={40}
              />
            ) : null}
            <View style={{ flex: 1, gap: 6 }}>
              <Text style={{ color: colors.text, fontWeight: '600', fontSize: 16 }}>
                {plain(item.title_snippet || item.title)}
              </Text>
              <Text
                numberOfLines={3}
                style={{ color: colors.secondary, fontSize: 15, lineHeight: 22 }}
              >
                {plain(item.snippet)}
              </Text>
              <Text style={{ color: colors.tertiary, fontSize: 12 }}>
                {item.space_title ??
                  (item.kind === 'person'
                    ? t('chat.person')
                    : item.kind === 'agent'
                      ? t('chat.agentLabel')
                      : t('chat.file'))}
              </Text>
            </View>
          </Pressable>
        )}
        ListFooterComponent={
          results.cursor ? (
            <View style={{ padding: 20 }}>
              <NativeButton
                secondary
                disabled={opening || results.loading || !results.completed}
                label={t('chat.moreResults')}
                onPress={() => {
                  void search.loadMore(results);
                }}
              />
            </View>
          ) : null
        }
      />
    </KeyboardAvoidingView>
  );
}
