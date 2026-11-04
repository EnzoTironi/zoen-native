import { SearchFilterRow } from '../components/search-filter-row';
import { SearchPeriodFilter } from '../components/search-period-filter';
import { SearchIdentityFilters } from '../components/search-identity-filters';
import { searchIdentityOptions } from '../core/search-identity-options';
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
import { messageDestination } from '../core/message-link';
import type { SearchPageInput } from '../core/generated/SearchPageInput';
import type { UniversalHit } from '../core/generated/UniversalHit';
import { useTheme } from '../theme';
import { EmptyState, Loading, NativeButton } from '../components/primitives';

const plain = (text: string) => text.replace(/\[\[|\]\]/g, '');

const searchFilters: {
  label: 'chat.all' | 'chat.searchMessagesFilter' | 'chat.people';
  kinds: string[];
}[] = [
  { label: 'chat.all', kinds: [] },
  { label: 'chat.searchMessagesFilter', kinds: ['message'] },
  { label: 'chat.people', kinds: ['person', 'agent'] },
];

const nativeSearchFilters = [
  ...searchFilters,
  { label: 'chat.searchMediaFilter' as const, kinds: ['media'] },
];

const mediaFilters = [
  { label: 'chat.searchAllMedia' as const, kinds: ['media'] },
  { label: 'chat.searchPhotos' as const, kinds: ['media:photo'] },
  { label: 'chat.searchVideos' as const, kinds: ['media:video'] },
  { label: 'chat.searchAudio' as const, kinds: ['media:audio'] },
  { label: 'chat.searchVoice' as const, kinds: ['media:voice'] },
  { label: 'chat.searchFiles' as const, kinds: ['media:file'] },
];
const searchPeriods = [
  { label: 'chat.searchAnyTime', days: 0 },
  { label: 'chat.searchLastWeek', days: 7 },
  { label: 'chat.searchLastMonth', days: 30 },
] as const;

function recentPeriod(days: number): SearchPageInput['period'] {
  if (!days) return undefined;
  const start = new Date();
  start.setHours(0, 0, 0, 0);
  const end = new Date(start);
  start.setDate(start.getDate() - days + 1);
  end.setDate(end.getDate() + 1);
  return { start_ms: start.getTime(), end_ms: end.getTime() };
}

export default function Search() {
  const params = useLocalSearchParams<{ space?: string; query?: string }>();
  const { store } = useClient();
  const space = params.space;
  const initial = params.query;
  const scope = searchScope(store, space);
  const owner = JSON.stringify([
    store.sessionEpoch,
    homeOwnerFor(store.getSnapshot()),
    store.getSnapshot().snapshot?.account?.relay_url ?? null,
  ]);
  const [initialOwner] = useState(owner);
  return (
    <SearchBody
      key={JSON.stringify([owner, space, Boolean(scope), initial])}
      space={space}
      initial={scope && owner === initialOwner ? initial : undefined}
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
  const [filter, setFilter] = useState({
    index: 0,
    media: 0,
    query: initial,
    days: 0,
    period: undefined as SearchPageInput['period'],
    messages: undefined as SearchPageInput['messages'],
    labels: {} as { author?: string; space?: string },
  });
  const supportsNativeFilters =
    process.env.EXPO_OS !== 'web' && store.getSnapshot().mode === 'account';
  const filters = supportsNativeFilters
    ? space
      ? nativeSearchFilters.filter(
          (item) => item.kinds.includes('message') || item.kinds.includes('media'),
        )
      : nativeSearchFilters
    : searchFilters;
  const filterIndex = filter.index < filters.length ? filter.index : 0;
  const isMedia = filters[filterIndex].kinds.includes('media');
  const isPeople = filters[filterIndex].kinds.includes('person');
  const { search, state: results } = useSearchResults(
    store,
    scope,
    space,
    filter.query,
    isMedia && supportsNativeFilters
      ? mediaFilters[filter.media].kinds
      : filters[filterIndex].kinds,
    supportsNativeFilters && !isPeople ? filter.period : undefined,
    supportsNativeFilters ? filter.messages : undefined,
  );
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
    if (
      !results.active ||
      results.loading ||
      results.completed ||
      results.error ||
      (!query.trim() && !search.allowsEmptyQuery)
    )
      return;
    const timer = setTimeout(() => void search.search(), 200);
    return () => clearTimeout(timer);
  }, [search, query, results.active, results.loading, results.completed, results.error]);
  const hits = scope ? results.hits : [];
  const readable = homeSpacesFor(store.getSnapshot());
  const choices = searchIdentityOptions(
    store.getSnapshot().snapshot?.conversations.filter((chat) => readable.has(chat.id)) ?? [],
    hits,
    space ?? filter.messages?.space_id,
  );
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
        if (
          hit.kind === 'message' &&
          (!destination || destination.id !== hit.ref_id || destination.deleted)
        )
          throw new Error(t('chat.thisResultCouldNotBeOpened'));
        await KeyboardController.dismiss();
        return destination;
      },
      (message) => {
        if (hit.space_id && !homeSpacesFor(store.getSnapshot()).has(hit.space_id)) return;
        if (hit.kind === 'message' && hit.space_id && message) {
          router.push(messageDestination(hit.space_id, message));
        } else if (hit.kind === 'space')
          router.push({ pathname: '/chat/[id]', params: { id: hit.ref_id } });
        else if (hit.kind === 'person' || hit.kind === 'agent')
          router.push({ pathname: '/person/[id]', params: { id: hit.ref_id } });
        else router.push({ pathname: '/item/[id]', params: { id: hit.ref_id } });
      },
      t('chat.thisResultCouldNotBeOpened'),
    );
  }
  const pageFailed = Boolean(
    results.cursor && !results.completed && results.error && !operation.error,
  );
  const errorNotice = error ? (
    <View style={{ paddingHorizontal: 24, paddingBottom: 16, gap: 12 }}>
      <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
        {error}
      </Text>
      <NativeButton
        secondary
        label={t('chat.tryAgain2')}
        disabled={!queryFits}
        onPress={() => {
          if (operation.error) {
            void actions.retry();
            return;
          }
          if (results === search.getSnapshot() && search.isCurrent()) void search.search();
        }}
      />
    </View>
  ) : null;
  const empty = !scope ? (
    <EmptyState title={t('chat.chatUnavailable')} />
  ) : !query.trim() && !search.allowsEmptyQuery ? (
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
      {scope && (!space || supportsNativeFilters) ? (
        <SearchFilterRow
          options={filters.map(({ label }) => t(label))}
          selected={filterIndex}
          onSelect={(index) => {
            if (filterIndex === index || results !== search.getSnapshot() || !search.isCurrent())
              return;
            search.suspend();
            setFilter({
              ...filter,
              index,
              query,
              ...(filters[index].kinds.includes('person') ? { messages: undefined } : {}),
            });
          }}
        />
      ) : null}
      {scope && supportsNativeFilters && isMedia ? (
        <SearchFilterRow
          horizontal
          options={mediaFilters.map(({ label }) => t(label))}
          selected={filter.media}
          onSelect={(media) => {
            if (filter.media === media || results !== search.getSnapshot() || !search.isCurrent())
              return;
            search.suspend();
            setFilter({ ...filter, query, media });
          }}
        />
      ) : null}
      {scope && supportsNativeFilters && !isPeople ? (
        <SearchFilterRow
          horizontal
          trailing={
            <SearchPeriodFilter
              value={filter.period ?? undefined}
              selected={filter.days === -1}
              isCurrent={() => results === search.getSnapshot() && search.isCurrent()}
              onChange={(period) => {
                if (results !== search.getSnapshot() || !search.isCurrent()) return;
                search.suspend();
                setFilter({ ...filter, query, period, days: -1 });
              }}
            />
          }
          options={searchPeriods.map(({ label }) => t(label))}
          selected={searchPeriods.findIndex(({ days }) => days === filter.days)}
          onSelect={(index) => {
            const { days } = searchPeriods[index];
            if (filter.days === days || results !== search.getSnapshot() || !search.isCurrent())
              return;
            search.suspend();
            setFilter({ ...filter, days, query, period: recentPeriod(days) });
          }}
        />
      ) : null}
      {scope && supportsNativeFilters && !isPeople ? (
        <SearchIdentityFilters
          value={filter.messages}
          labels={filter.labels}
          options={choices}
          fixedSpace={Boolean(space)}
          isCurrent={() => results === search.getSnapshot() && search.isCurrent()}
          onChange={(messages) => {
            if (results !== search.getSnapshot() || !search.isCurrent()) return;
            search.suspend();
            setFilter({
              ...filter,
              query,
              messages,
              labels: {
                author:
                  choices.authors.find((item) => item.id === messages?.author_id)?.title ??
                  (messages?.author_id === filter.messages?.author_id
                    ? filter.labels.author
                    : undefined),
                space: choices.spaces.find((item) => item.id === messages?.space_id)?.title,
              },
            });
          }}
        />
      ) : null}
      <FlashList
        testID="global-search-results"
        contentInsetAdjustmentBehavior="never"
        contentContainerStyle={{ flexGrow: 1, paddingBottom: insets.bottom + 16 }}
        data={hits}
        keyExtractor={(hit) => `${hit.kind}:${hit.space_id}:${hit.ref_id}`}
        keyboardShouldPersistTaps="handled"
        keyboardDismissMode={process.env.EXPO_OS === 'ios' ? 'interactive' : 'on-drag'}
        ListHeaderComponent={pageFailed ? null : errorNotice}
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
          pageFailed ? (
            errorNotice
          ) : results.cursor ? (
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
