import { Host, Picker, Switch } from '@expo/ui';
import { useState } from 'react';
import { Text, View } from 'react-native';
import type { InboxFolder } from '../core/generated/InboxFolder';
import { SettingsScreen } from '../components/settings-screen';
import { SettingsRows } from '../components/settings-rows';
import { Field, NativeButton } from '../components/primitives';
import { useTheme } from '../theme';
import { t } from '../i18n/index.ts';
import { builtinFolders, folderTabs, newFolder, withoutFolder } from './folders';
import { useFolderEditor } from './use-folder-editor';

export function FolderSettings() {
  const editor = useFolderEditor(),
    { owner, base, blocked, pending, change } = editor;
  const { presentation: p, scheme } = useTheme();
  const [editing, setEditing] = useState<{ owner: string; id: string } | null>(null);
  const [search, setSearch] = useState('');
  const [visible, setVisible] = useState(50);
  const [list, setList] = useState<'include' | 'exclude'>('include');
  const selected = base?.configuration.folders.find(
    (folder) => editing?.owner === owner && folder.id === editing.id,
  );
  const update = (patch: Partial<InboxFolder>) => {
    if (selected && base)
      change({
        ...base.configuration,
        folders: base.configuration.folders.map((folder) =>
          folder.id === selected.id ? { ...folder, ...patch } : folder,
        ),
      });
  };
  const add = async () => {
    const id = await editor.newId();
    if (!id || !base) return;
    const folder = newFolder(id);
    change({ ...base.configuration, folders: [...base.configuration.folders, folder] });
    setEditing({ owner, id: folder.id });
    setSearch('');
  };
  const move = (direction: number) => {
    if (!selected || !base) return;
    const folders = [...base.configuration.folders],
      from = folders.findIndex((f) => f.id === selected.id),
      to = from + direction;
    if (to < 0 || to >= folders.length) return;
    [folders[from], folders[to]] = [folders[to], folders[from]];
    change({ ...base.configuration, folders });
  };
  const chats = editor.chats.filter((chat) =>
    chat.title.toLocaleLowerCase().includes(search.toLocaleLowerCase()),
  );
  const toggle = (
    label: string,
    value: boolean,
    onChange: (next: boolean) => void,
    id: string,
    limited = false,
  ) => (
    <View key={id} style={{ minHeight: 50, paddingHorizontal: 16, justifyContent: 'center' }}>
      <Host
        matchContents={{ vertical: true }}
        style={{ width: '100%' }}
        colorScheme={scheme}
        seedColor={p.accent}
      >
        <Switch
          testID={id}
          label={label}
          value={value}
          onValueChange={onChange}
          disabled={blocked || limited}
        />
      </Host>
    </View>
  );
  const blocks = { backgroundColor: p.list.block, borderRadius: 10 };
  return (
    <SettingsScreen
      title={selected ? selected.name || t('inboxActions.addFolder') : t('inboxActions.folders')}
      testID="folder-settings"
      automaticallyAdjustKeyboardInsets
    >
      {!base ? (
        <Text accessibilityRole="alert" style={{ color: p.destructive }}>
          {t('inboxActions.foldersUnavailable')}
        </Text>
      ) : (
        <>
          {selected ? (
            <>
              <Field
                label={t('inboxActions.folderName')}
                value={selected.name}
                onChangeText={(name) => update({ name })}
                maxLength={64}
                editable={!blocked}
                testID="folder-name"
              />
              <View style={blocks}>
                {(['Direct', 'Group', 'Community'] as const).map((kind) =>
                  toggle(
                    builtinFolders().find((tab) => tab.id === kind)?.label ?? kind,
                    selected.kinds.includes(kind),
                    (value) =>
                      update({
                        kinds: value
                          ? [...selected.kinds, kind]
                          : selected.kinds.filter((item) => item !== kind),
                      }),
                    `folder-kind-${kind}`,
                  ),
                )}
              </View>
              <View style={blocks}>
                {(
                  [
                    ['exclude_read', 'excludeRead'],
                    ['exclude_muted', 'excludeMuted'],
                    ['exclude_archived', 'excludeArchived'],
                  ] as const
                ).map(([key, label]) =>
                  toggle(
                    t(`inboxActions.${label}`),
                    selected[key],
                    (value) => update({ [key]: value }),
                    `folder-${key}`,
                  ),
                )}
              </View>
              <Host
                matchContents={{ vertical: true }}
                style={{ width: '100%' }}
                colorScheme={scheme}
              >
                <Picker
                  testID="folder-membership-mode"
                  selectedValue={list}
                  onValueChange={setList}
                  appearance="menu"
                >
                  <Picker.Item label={t('inboxActions.includedChats')} value="include" />
                  <Picker.Item label={t('inboxActions.excludedChats')} value="exclude" />
                </Picker>
              </Host>
              <Field
                label={t('chat.search')}
                value={search}
                onChangeText={(value) => {
                  setSearch(value);
                  setVisible(50);
                }}
                testID="folder-chat-search"
              />
              <View style={blocks}>
                {chats.slice(0, visible).map((chat) =>
                  toggle(
                    chat.title,
                    selected[list].includes(chat.id),
                    (value) =>
                      update({
                        [list]: value
                          ? [...selected[list], chat.id]
                          : selected[list].filter((id) => id !== chat.id),
                        ...(value
                          ? {
                              [list === 'include' ? 'exclude' : 'include']: selected[
                                list === 'include' ? 'exclude' : 'include'
                              ].filter((id) => id !== chat.id),
                            }
                          : {}),
                      }),
                    `folder-chat-${chat.id}`,
                    !selected[list].includes(chat.id) && selected[list].length >= 200,
                  ),
                )}
              </View>
              {visible < chats.length ? (
                <NativeButton
                  label={t('inboxActions.moreChats')}
                  secondary
                  onPress={() => setVisible((count) => count + 50)}
                />
              ) : null}
              <View style={{ flexDirection: 'row', gap: 16, flexWrap: 'wrap' }}>
                <NativeButton
                  label={t('inboxActions.moveUp')}
                  secondary
                  disabled={blocked || base.configuration.folders[0].id === selected.id}
                  onPress={() => move(-1)}
                />
                <NativeButton
                  label={t('inboxActions.moveDown')}
                  secondary
                  disabled={blocked || base.configuration.folders.at(-1)?.id === selected.id}
                  onPress={() => move(1)}
                />
              </View>
              <NativeButton
                label={t('inboxActions.removeFolder')}
                destructive
                disabled={blocked}
                onPress={() => {
                  change(withoutFolder(base.configuration, selected.id));
                  setEditing(null);
                }}
              />
              <NativeButton
                label={t('common.back')}
                secondary
                onPress={() => setEditing(null)}
                disabled={pending}
              />
            </>
          ) : (
            <>
              <Text style={{ color: p.list.section, fontSize: 13 }}>
                {t('inboxActions.initialFolder')}
              </Text>
              <Host
                matchContents={{ vertical: true }}
                style={{ width: '100%' }}
                colorScheme={scheme}
              >
                <Picker
                  testID="folder-initial"
                  selectedValue={base.configuration.default_folder}
                  onValueChange={(default_folder: string) =>
                    change({ ...base.configuration, default_folder })
                  }
                  appearance="menu"
                >
                  {folderTabs(base.configuration).map((folder) => (
                    <Picker.Item key={folder.id} label={folder.label} value={folder.id} />
                  ))}
                </Picker>
              </Host>
              <SettingsRows
                testID="folder-list"
                rows={base.configuration.folders.map((folder) => ({
                  id: `folder-edit-${folder.id}`,
                  title: folder.name || t('inboxActions.addFolder'),
                  disabled: blocked,
                  icon: { ios: 'folder', android: 'folder', web: 'folder' },
                  onPress: () => {
                    setEditing({ owner, id: folder.id });
                    setSearch('');
                  },
                }))}
              />
              <NativeButton
                label={t('inboxActions.addFolder')}
                onPress={() => {
                  void add();
                }}
                disabled={blocked || base.configuration.folders.length >= 20}
                testID="folder-add"
              />
            </>
          )}
          {editor.error ? (
            <Text accessibilityRole="alert" selectable style={{ color: p.destructive }}>
              {editor.error}
            </Text>
          ) : null}
          <NativeButton
            label={t('inboxActions.saveFolders')}
            onPress={() => {
              void editor.save();
            }}
            disabled={
              pending ||
              !editor.canSave ||
              base.configuration.folders.some(
                (folder) =>
                  !folder.name.trim() ||
                  folder.name !== folder.name.trim() ||
                  Array.from(folder.name).length > 32,
              )
            }
            testID="folder-save"
          />
          <NativeButton
            label={t('common.discard')}
            secondary
            onPress={() => {
              editor.discard();
              setEditing(null);
            }}
            disabled={pending}
            testID="folder-discard"
          />
        </>
      )}
    </SettingsScreen>
  );
}
