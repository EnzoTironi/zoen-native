import { Host, Picker } from '@expo/ui';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { useRef, useState } from 'react';
import { ScrollView, Text } from 'react-native';
import { Field, NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import { DocumentExportActivity } from '../documents/export-activity';
import { useDirtyEditor } from '../documents/use-dirty-editor';
import { useFocusEffect } from 'expo-router';
import { useCallback } from 'react';
import { siteAuthority, siteFilename, siteScope, emptySite } from '../sites/source';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';

export default function NewSite() {
  const { space: initial } = useLocalSearchParams<{ space?: string }>();
  const { store, snapshot } = useClient();
  const { colors, scheme } = useTheme();
  const [space, setSpace] = useState(initial ?? '');
  const [title, setTitle] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activity] = useState(() => new DocumentExportActivity());
  const mutation = useRef<{ fingerprint: string; id: string } | null>(null);
  const finish = useDirtyEditor(!!title.trim(), busy);
  const scope = siteScope(store, space);
  const chats =
    snapshot?.conversations.filter(
      (chat) => chat.privacy === 'EndToEnd' && siteScope(store, chat.id),
    ) ?? [];
  useFocusEffect(
    useCallback(() => {
      setBusy(false);
      return activity.focus();
    }, [activity]),
  );
  async function create() {
    if (!scope || !title.trim() || busy) return;
    const authority = siteAuthority(store, scope);
    const ticket = activity.begin();
    if (!authority || !ticket) return;
    const current = () => ticket.current() && siteAuthority(store, scope) === authority;
    setBusy(true);
    setError(null);
    const name = siteFilename(title);
    const html = emptySite(title.trim());
    const fingerprint = JSON.stringify([authority, space, name, html]);
    try {
      if (mutation.current?.fingerprint !== fingerprint) {
        const id = await store.driver.request({ op: 'new_mutation_id' });
        if (!current()) return;
        mutation.current = { fingerprint, id };
      }
      const result = await store.request({
        op: 'site_create',
        scope,
        name,
        html,
        mutation_id: mutation.current!.id,
      });
      if (!current() || result.source.space_id !== space || result.html !== html) return;
      finish();
      router.replace({ pathname: '/site/[id]', params: { id: result.source.item_id } });
    } catch (reason) {
      if (current()) setError(reason instanceof Error ? reason.message : t('sites.failedSave'));
    } finally {
      if (ticket.finish()) setBusy(false);
    }
  }
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{ padding: 24, gap: 24 }}
    >
      <Stack.Screen options={{ title: t('sites.newSite') }} />
      <Field
        label={t('sites.name')}
        value={title}
        onChangeText={setTitle}
        editable={!busy}
        autoFocus
      />
      <Host matchContents colorScheme={scheme} seedColor={colors.accent}>
        <Picker selectedValue={space} enabled={!busy} onValueChange={setSpace}>
          <Picker.Item label={t('sites.chooseChat')} value="" />
          {chats.map((chat) => (
            <Picker.Item key={chat.id} label={chat.title} value={chat.id} />
          ))}
        </Picker>
      </Host>
      {error ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {error}
        </Text>
      ) : null}
      <NativeButton
        label={busy ? t('sites.saving') : t('sites.create')}
        disabled={busy || !scope || !title.trim()}
        onPress={() => void create()}
      />
    </ScrollView>
  );
}
