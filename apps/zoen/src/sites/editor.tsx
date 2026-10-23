import { Host, Picker } from '@expo/ui';
import { Stack, router, useFocusEffect } from 'expo-router';
import { useHeaderHeight } from 'expo-router/react-navigation';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useCallback, useState } from 'react';
import { KeyboardAvoidingView, Text, TextInput, View } from 'react-native';
import { EmptyState, NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import type { SiteScope } from '../core/generated/SiteScope';
import type { SiteSource } from '../core/generated/SiteSource';
import { DocumentExportActivity } from '../documents/export-activity';
import { useDirtyEditor } from '../documents/use-dirty-editor';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { SitePreview } from './preview';
import { sameSource, siteAuthority, validSiteText } from './source';
import { saveObservedSite } from './actions';

export function SiteEditor({
  initial,
  scope,
  authority,
  available,
  refresh,
}: {
  initial: SiteSource;
  scope: SiteScope;
  authority: string;
  available: boolean;
  refresh: () => void;
}) {
  const { store } = useClient();
  const { colors, scheme } = useTheme();
  const headerHeight = useHeaderHeight();
  const insets = useSafeAreaInsets();
  const [saved, setSaved] = useState(initial);
  const [html, setHtml] = useState(initial.html ?? '');
  const [mode, setMode] = useState('preview');
  const [focused, setFocused] = useState(false);
  const [activity] = useState(() => new DocumentExportActivity());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dirty = saved.html !== null && html !== saved.html;
  useDirtyEditor(dirty, busy);
  useFocusEffect(
    useCallback(() => {
      setFocused(true);
      setBusy(false);
      const end = activity.focus();
      return () => {
        setFocused(false);
        end();
      };
    }, [activity]),
  );
  const current = () => focused && available && siteAuthority(store, scope) === authority;
  async function save() {
    if (!current() || busy || !saved.editable || !dirty) return;
    if (!validSiteText(html)) {
      setError(t('sites.tooLarge'));
      return;
    }
    const ticket = activity.begin();
    if (!ticket) return;
    const source = saved.source;
    const text = html;
    setBusy(true);
    setError(null);
    try {
      const next = await saveObservedSite(store, scope, authority, source, text, () => ticket.current() && current());
      if (!next) return;
      setSaved(next);
      setHtml(next.html ?? '');
    } catch (reason) {
      if (ticket.current() && current())
        setError(reason instanceof Error ? reason.message : t('sites.failedSave'));
    } finally {
      if (ticket.finish() && current()) setBusy(false);
    }
  }
  async function download() {
    if (!current() || busy) return;
    const ticket = activity.begin();
    if (!ticket) return;
    setBusy(true);
    setError(null);
    try {
      const local = await store.driver.media.materializeFile(
        saved.source.item_id,
        saved.source.version,
      );
      try {
        if (!ticket.current() || !current()) return;
        const result = await store.driver.request({
          op: 'site_read',
          scope,
          item_id: saved.source.item_id,
          version: saved.source.version,
          expected_sha256: saved.source.sha256,
        });
        if (!ticket.current() || !current()) return;
        if (!sameSource(saved.source, result.source) || result.html === null)
          throw new Error(t('sites.unavailable'));
        setSaved(result);
        setHtml(result.html);
      } finally {
        await store.driver.media.releaseLocalFile(local.uri);
      }
    } catch (reason) {
      if (ticket.current() && current())
        setError(reason instanceof Error ? reason.message : t('sites.unavailable'));
    } finally {
      if (ticket.finish() && current()) setBusy(false);
    }
  }
  if (!available || siteAuthority(store, scope) !== authority)
    return (
      <EmptyState title={t('sites.unavailable')}>
        <NativeButton label={t('sites.retry')} onPress={refresh} />
      </EmptyState>
    );
  return (
    <KeyboardAvoidingView
      behavior={process.env.EXPO_OS === 'ios' ? 'padding' : undefined}
      style={{ flex: 1, paddingTop: headerHeight, paddingBottom: insets.bottom }}
    >
      <Stack.Screen
        options={{
          title: saved.name,
          headerRight: () => (
            <NativeButton
              label={busy ? t('sites.saving') : t('sites.save')}
              disabled={!dirty || busy || !saved.editable}
              onPress={() => void save()}
            />
          ),
        }}
      />
      <View style={{ paddingHorizontal: 16, paddingVertical: 10, gap: 10 }}>
        <Host matchContents colorScheme={scheme} seedColor={colors.accent}>
          <Picker selectedValue={mode} onValueChange={setMode}>
            <Picker.Item label={t('sites.preview')} value="preview" />
            <Picker.Item label={t('sites.source')} value="source" />
          </Picker>
        </Host>
        {error ? (
          <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
            {error}
          </Text>
        ) : null}
      </View>
      {saved.html === null ? (
        <EmptyState title={t('sites.version', { version: saved.source.version })}>
          <NativeButton
            label={busy ? t('sites.downloading') : t('sites.download')}
            disabled={busy}
            onPress={() => void download()}
          />
        </EmptyState>
      ) : mode === 'preview' ? (
        <SitePreview
          html={html}
          ownerKey={`${authority}:${saved.source.item_id}`}
          active={focused}
        />
      ) : (
        <TextInput
          testID="site-source"
          accessibilityLabel={t('sites.sourceLabel')}
          multiline
          value={html}
          onChangeText={setHtml}
          editable={saved.editable && !busy}
          autoCapitalize="none"
          autoCorrect={false}
          textAlignVertical="top"
          scrollEnabled
          style={{
            flex: 1,
            padding: 18,
            fontFamily: 'Menlo',
            fontSize: 14,
            lineHeight: 21,
            color: colors.text,
          }}
        />
      )}
      <View style={{ paddingHorizontal: 16, paddingVertical: 8 }}>
        <NativeButton
          secondary
          label={t('sites.history')}
          disabled={busy}
          onPress={() =>
            router.push({
              pathname: '/item/[id]',
              params: { id: saved.source.item_id, history: '1' },
            })
          }
        />
      </View>
    </KeyboardAvoidingView>
  );
}
