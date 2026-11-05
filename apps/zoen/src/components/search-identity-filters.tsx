import { BottomSheet, Host, RNHostView } from '@expo/ui';
import { useLayoutEffect, useRef, useState } from 'react';
import { FlatList, Pressable, Text, TextInput, View } from 'react-native';
import type { MessageSearchFilter } from '../core/generated/MessageSearchFilter';
import type { SearchIdentityOption } from '../core/search-identity-options';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { ProfileAvatar } from '../profile/profile-avatar';
import { Icon, IconButton } from './primitives';

type Props = {
  value: MessageSearchFilter | undefined;
  labels?: { author?: string; space?: string };
  options: { spaces: readonly SearchIdentityOption[]; authors: readonly SearchIdentityOption[] };
  fixedSpace?: boolean;
  isCurrent(): boolean;
  onChange(value: MessageSearchFilter | undefined): void;
};
/** Telegram's author domain and chat identity are selectors, never query-string heuristics. */
export function SearchIdentityFilters({
  value,
  labels,
  options,
  fixedSpace,
  isCurrent,
  onChange,
}: Props) {
  const { colors, scheme } = useTheme();
  const [panel, setPanel] = useState<{ kind: 'author_id' | 'space_id'; query: string } | null>(
    null,
  );
  const livePanel = useRef(panel);
  useLayoutEffect(() => {
    livePanel.current = panel;
    return () => {
      livePanel.current = null;
    };
  });
  const close = () => {
    livePanel.current = null;
    setPanel(null);
  };
  const author = options.authors.find((item) => item.id === value?.author_id);
  const space = options.spaces.find((item) => item.id === value?.space_id);
  const all = panel?.kind === 'author_id' ? options.authors : options.spaces;
  const query = panel?.query.trim().toLocaleLowerCase() ?? '';
  const rows = all.filter(
    (item) =>
      !query || `${item.title} ${item.person?.handle ?? ''}`.toLocaleLowerCase().includes(query),
  );
  const choose = (id: string | null) => {
    if (
      !panel ||
      livePanel.current !== panel ||
      !isCurrent() ||
      (id && !all.some((item) => item.id === id))
    )
      return;
    const next = { ...value };
    if (id) next[panel.kind] = id;
    else delete next[panel.kind];
    close();
    onChange(Object.keys(next).length ? next : undefined);
  };
  const chip = (kind: 'author_id' | 'space_id', label: string, selected: boolean) => (
    <Pressable
      testID={`search-filter-${kind}`}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityState={{ selected }}
      onPress={() => {
        if (isCurrent()) setPanel({ kind, query: '' });
      }}
      style={{
        flexDirection: 'row',
        alignItems: 'center',
        gap: 6,
        minHeight: 44,
        paddingHorizontal: 14,
        borderRadius: 22,
        backgroundColor: selected ? colors.muted : 'transparent',
      }}
    >
      <Text numberOfLines={1} style={{ color: colors.accent, maxWidth: 190 }}>
        {label}
      </Text>
      <Icon
        name={{ ios: 'chevron.down', android: 'expand_more', web: 'expand_more' }}
        size={12}
        color={colors.accent}
      />
    </Pressable>
  );
  return (
    <>
      <View style={{ flexDirection: 'row', flexWrap: 'wrap', marginHorizontal: 20, gap: 8 }}>
        {chip(
          'author_id',
          author?.title || (value?.author_id ? labels?.author : undefined) || t('chat.searchFrom'),
          Boolean(value?.author_id),
        )}
        {!fixedSpace
          ? chip(
              'space_id',
              space?.title || (value?.space_id ? labels?.space : undefined) || t('chat.searchIn'),
              Boolean(value?.space_id),
            )
          : null}
      </View>
      <Host
        style={{ position: 'absolute' }}
        pointerEvents="none"
        colorScheme={scheme}
        seedColor={colors.accent}
      >
        <BottomSheet
          testID="search-identity-sheet"
          isPresented={Boolean(panel)}
          onDismiss={close}
          snapPoints={['half', 'full']}
          containerColor={colors.background}
        >
          <RNHostView>
            <View style={{ flex: 1, paddingTop: 16 }}>
              <View style={{ flexDirection: 'row', alignItems: 'center', paddingHorizontal: 20 }}>
                <Text
                  accessibilityRole="header"
                  style={{ flex: 1, fontSize: 20, fontWeight: '600', color: colors.text }}
                >
                  {t(panel?.kind === 'author_id' ? 'chat.searchFrom' : 'chat.searchIn')}
                </Text>
                <IconButton
                  name={{ ios: 'xmark', android: 'close', web: 'close' }}
                  label={t('common.close')}
                  onPress={close}
                />
              </View>
              <TextInput
                testID="search-identity-query"
                accessibilityLabel={t('chat.search')}
                value={panel?.query ?? ''}
                onChangeText={(query) =>
                  setPanel((current) => (current ? { ...current, query } : null))
                }
                placeholder={t('chat.search')}
                placeholderTextColor={colors.secondary}
                autoCorrect={false}
                style={{
                  margin: 16,
                  minHeight: 44,
                  paddingHorizontal: 14,
                  borderRadius: 12,
                  color: colors.text,
                  backgroundColor: colors.muted,
                }}
              />
              <FlatList<SearchIdentityOption>
                testID="search-identity-options"
                data={rows}
                keyExtractor={(item) => item.id}
                keyboardShouldPersistTaps="handled"
                keyboardDismissMode="on-drag"
                contentContainerStyle={{ paddingBottom: 32 }}
                ListHeaderComponent={
                  <Pressable
                    testID="search-identity-clear"
                    accessibilityRole="button"
                    onPress={() => choose(null)}
                    style={{ paddingHorizontal: 20, minHeight: 48, justifyContent: 'center' }}
                  >
                    <Text style={{ color: colors.accent }}>{t('chat.all')}</Text>
                  </Pressable>
                }
                renderItem={({ item }) => (
                  <Pressable
                    testID={`search-identity-${item.id}`}
                    accessibilityRole="button"
                    accessibilityState={{
                      selected: Boolean(panel && value?.[panel.kind] === item.id),
                    }}
                    onPress={() => choose(item.id)}
                    style={{
                      flexDirection: 'row',
                      alignItems: 'center',
                      gap: 12,
                      paddingHorizontal: 20,
                      paddingVertical: 10,
                      minHeight: 52,
                    }}
                  >
                    {item.person ? (
                      <ProfileAvatar
                        person={item.person}
                        name={item.title}
                        color={item.person.tint_hex}
                        agent={item.person.kind === 'Agent'}
                        size={40}
                      />
                    ) : null}
                    <View style={{ flex: 1 }}>
                      <Text style={{ color: colors.text, fontSize: 17 }}>{item.title}</Text>
                      {item.person?.handle ? (
                        <Text style={{ color: colors.secondary, fontSize: 14 }}>
                          @{item.person.handle}
                        </Text>
                      ) : null}
                    </View>
                    {panel && value?.[panel.kind] === item.id ? (
                      <Icon
                        name={{ ios: 'checkmark', android: 'check', web: 'check' }}
                        size={20}
                        color={colors.accent}
                      />
                    ) : null}
                  </Pressable>
                )}
              />
            </View>
          </RNHostView>
        </BottomSheet>
      </Host>
    </>
  );
}
