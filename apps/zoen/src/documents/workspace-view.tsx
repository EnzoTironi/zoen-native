import { Host, Picker } from '@expo/ui';
import { FlashList } from '@shopify/flash-list';
import { useState } from 'react';
import { Text, TextInput, View } from 'react-native';
import { EmptyState, Loading, NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import { useCoreQuery } from '../core/query';
import { homeSpacesFor } from '../home/access';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { workspaceKinds, workspaceRows, type WorkspaceKind } from './workspace-index';
import { WorkspaceItemRow } from './workspace-row';
import { WorkspaceCreate } from './workspace-create';

/** A view of canonical Items: folders and search never create a second document store. */
export function WorkspaceBrowser() {
  const state = useClient();
  const { colors, presentation: p, scheme } = useTheme();
  const read = useCoreQuery({ op: 'items' });
  const [query, setQuery] = useState('');
  const [kind, setKind] = useState<WorkspaceKind>('all');
  const [selectedSpace, setSpace] = useState('');
  const [folder, setFolder] = useState('');
  const spaces = homeSpacesFor(state);
  const space = spaces.has(selectedSpace) ? selectedSpace : '';
  const currentFolder = space ? folder : '';
  const rows = workspaceRows(read.data ?? [], {
    spaces,
    space: space || null,
    kind,
    query,
    folder: currentFolder,
  });
  const filtered = !!(query.trim() || space || kind !== 'all' || currentFolder);
  const chats = state.snapshot?.conversations.filter((chat) => spaces.has(chat.id)) ?? [];
  function clear() {
    setQuery('');
    setKind('all');
    setSpace('');
    setFolder('');
  }
  return (
    <FlashList
      data={rows}
      keyExtractor={(row) => row.key}
      getItemType={(row) => row.type}
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      keyboardDismissMode="on-drag"
      contentContainerStyle={{ paddingHorizontal: 20, paddingBottom: 28 }}
      refreshing={read.loading && read.data !== null}
      onRefresh={read.retry}
      ListHeaderComponent={
        <View style={{ paddingTop: 20, paddingBottom: 12, gap: 16 }}>
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
            <Text
              accessibilityRole="header"
              style={{
                flex: 1,
                color: colors.text,
                fontSize: 34,
                fontWeight: '700',
                letterSpacing: -1,
              }}
            >
              {t('workspace.title')}
            </Text>
            <WorkspaceCreate space={space} />
          </View>
          <TextInput
            testID="workspace-search"
            accessibilityLabel={t('workspace.search')}
            placeholder={t('workspace.search')}
            placeholderTextColor={p.search.placeholder}
            value={query}
            onChangeText={setQuery}
            autoCapitalize="none"
            autoCorrect={false}
            clearButtonMode="while-editing"
            returnKeyType="search"
            style={{
              minHeight: 44,
              paddingHorizontal: 14,
              paddingVertical: 10,
              color: colors.text,
              backgroundColor: p.search.background,
              borderRadius: 12,
              fontSize: 17,
            }}
          />
          <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 12, alignItems: 'center' }}>
            <View style={{ gap: 4, maxWidth: '100%' }}>
              <Text style={{ color: colors.secondary, fontSize: 12 }}>{t('workspace.scope')}</Text>
              <Host matchContents colorScheme={scheme} seedColor={colors.accent}>
                <Picker
                  selectedValue={space}
                  onValueChange={(value) => {
                    setSpace(value);
                    setFolder('');
                  }}
                  testID="workspace-chat"
                >
                  <Picker.Item label={t('workspace.allChats')} value="" />
                  {chats.map((chat) => (
                    <Picker.Item key={chat.id} label={chat.title} value={chat.id} />
                  ))}
                </Picker>
              </Host>
            </View>
            <View style={{ gap: 4 }}>
              <Text style={{ color: colors.secondary, fontSize: 12 }}>{t('workspace.kind')}</Text>
              <Host matchContents colorScheme={scheme} seedColor={colors.accent}>
                <Picker selectedValue={kind} onValueChange={setKind} testID="workspace-kind">
                  {workspaceKinds.map((value) => (
                    <Picker.Item key={value} label={t(`workspace.${value}`)} value={value} />
                  ))}
                </Picker>
              </Host>
            </View>
          </View>
          {currentFolder ? (
            <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
              <IconButton
                name={{ ios: 'chevron.left', android: 'chevron_left', web: 'chevron_left' }}
                label={t('workspace.up')}
                onPress={() => setFolder(currentFolder.split('/').slice(0, -1).join('/'))}
              />
              <Text selectable style={{ color: colors.text, flex: 1, fontSize: 15 }}>
                {currentFolder}
              </Text>
              <NativeButton label={t('workspace.root')} secondary onPress={() => setFolder('')} />
            </View>
          ) : (
            <Text style={{ color: colors.secondary, fontSize: 13 }}>{t('workspace.recent')}</Text>
          )}
          {read.error ? (
            <View style={{ gap: 8 }}>
              <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
                {read.error}
              </Text>
              <NativeButton label={t('workspace.retry')} secondary onPress={read.retry} />
            </View>
          ) : null}
        </View>
      }
      ListEmptyComponent={
        read.data === null ? (
          read.loading ? (
            <Loading />
          ) : null
        ) : (
          <EmptyState
            title={t(filtered ? 'workspace.noResults' : 'workspace.empty')}
            detail={t(filtered ? 'workspace.noResultsDetail' : 'workspace.emptyDetail')}
            icon={{ ios: 'folder', android: 'folder', web: 'folder' }}
          >
            {filtered ? (
              <NativeButton label={t('workspace.reset')} secondary onPress={clear} />
            ) : null}
          </EmptyState>
        )
      }
      renderItem={({ item }) => <WorkspaceItemRow row={item} openFolder={setFolder} />}
    />
  );
}
