import { useEffect, useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { FlashList } from '@shopify/flash-list';
import { Pressable, Text, TextInput, View } from 'react-native';
import { useCoreQuery } from '../core/query';
import { useClient } from '../core/provider';
import type { UniversalHit } from '../core/generated/UniversalHit';
import { useTheme } from '../theme';
import { Avatar, EmptyState, Loading, NativeButton } from '../components/primitives';

const plain = (text: string) => text.replace(/\[\[|\]\]/g, '');
export default function Search() {
  const { space, query: initial } = useLocalSearchParams<{ space?: string; query?: string }>();
  const { store } = useClient();
  const { colors } = useTheme();
  const [query, setQuery] = useState(initial ?? '');
  const [term, setTerm] = useState(query);
  const [opening, setOpening] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [more, setMore] = useState<{
    base: string;
    hits: UniversalHit[];
    cursor: string | null;
  } | null>(null);
  useEffect(() => {
    const timer = setTimeout(() => setTerm(query.trim()), 200);
    return () => clearTimeout(timer);
  }, [query]);
  const global = useCoreQuery(
    term && !space ? { op: 'universal_search', query: term, kinds: [], limit: 30 } : null,
  );
  const scoped = useCoreQuery(
    term && space
      ? { op: 'search_messages', space_id: space, query: term, cursor: null, limit: 30 }
      : null,
  );
  const first = space ? scoped.data : global.data;
  const base = JSON.stringify(first);
  const extra = more?.base === base ? more : null;
  const hits = [...(first?.hits ?? []), ...(extra?.hits ?? [])];
  const cursor = extra ? extra.cursor : scoped.data?.next_cursor;
  const error = failure ?? (space ? scoped.error : global.error);
  async function open(hit: UniversalHit) {
    if (opening) return;
    setOpening(true);
    setFailure(null);
    try {
      if (hit.kind === 'message' && hit.space_id) {
        const message = await store.driver.request({
          op: 'message',
          space_id: hit.space_id,
          message_id: hit.ref_id,
        });
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
    } catch (reason) {
      setFailure(
        reason instanceof Error ? reason.message : 'Não foi possível abrir este resultado.',
      );
    } finally {
      setOpening(false);
    }
  }
  return (
    <View style={{ flex: 1, backgroundColor: colors.surface }}>
      <Stack.Screen options={{ title: space ? 'Buscar na conversa' : 'Buscar' }} />
      <TextInput
        accessibilityLabel="Buscar"
        autoFocus
        value={query}
        onChangeText={setQuery}
        autoCorrect={false}
        returnKeyType="search"
        placeholder={space ? 'Mensagens desta conversa' : 'Pessoas, mensagens, páginas…'}
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
      {error ? (
        <Text
          accessibilityRole="alert"
          selectable
          style={{ color: colors.danger, paddingHorizontal: 24, paddingBottom: 16 }}
        >
          {error}
        </Text>
      ) : null}
      {!term ? (
        <EmptyState
          title="Encontre o que ficou na conversa"
          detail="A busca acontece no histórico disponível neste aparelho."
          icon={{ ios: 'magnifyingglass', android: 'search', web: 'search' }}
        />
      ) : !first && !error ? (
        <Loading />
      ) : (
        <FlashList
          data={hits}
          keyExtractor={(hit) => `${hit.kind}:${hit.space_id}:${hit.ref_id}`}
          keyboardShouldPersistTaps="handled"
          keyboardDismissMode="on-drag"
          ListEmptyComponent={
            <EmptyState title="Nenhum resultado" detail="Tente outra palavra ou nome." />
          }
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
                <Avatar
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
                      ? 'Pessoa'
                      : item.kind === 'agent'
                        ? 'Agente'
                        : 'Arquivo')}
                </Text>
              </View>
            </Pressable>
          )}
          ListFooterComponent={
            space && cursor ? (
              <View style={{ padding: 20 }}>
                <NativeButton
                  secondary
                  disabled={opening}
                  label="Mais resultados"
                  onPress={() => {
                    setOpening(true);
                    setFailure(null);
                    void store.driver
                      .request({
                        op: 'search_messages',
                        space_id: space,
                        query: term,
                        cursor,
                        limit: 30,
                      })
                      .then((page) =>
                        setMore({
                          base,
                          hits: [...(extra?.hits ?? []), ...page.hits],
                          cursor: page.next_cursor,
                        }),
                      )
                      .catch((reason: unknown) =>
                        setFailure(
                          reason instanceof Error
                            ? reason.message
                            : 'A busca mudou. Tente novamente.',
                        ),
                      )
                      .finally(() => setOpening(false));
                  }}
                />
              </View>
            ) : null
          }
        />
      )}
    </View>
  );
}
