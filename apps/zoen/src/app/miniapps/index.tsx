import { binaryEditorFormat, isBinaryEditorId } from '../../miniapps/binary-formats';
import { t } from '../../i18n/index.ts';
import { Host, Picker } from '@expo/ui';
import { useRef, useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { ScrollView, Text } from 'react-native';
import { useClient } from '../../core/provider';
import { useCoreQuery } from '../../core/query';
import type { AppSpecDto } from '../../core/generated/AppSpecDto';
import { EmptyState, Loading, NativeButton, Surface } from '../../components/primitives';
import { useTheme } from '../../theme';
import { SchemaForm } from '../../miniapps/schema-form';
import { exactJson, formValue, parseSchema } from '../../miniapps/schema';
import { useDirtyEditor } from '../../documents/use-dirty-editor';
import { useDialogs } from '../../components/dialogs';
import { installIntent } from '../../miniapps/install-intent';

function Install({
  spec,
  initialSpace,
  onBack,
}: {
  spec: AppSpecDto;
  initialSpace: string;
  onBack: () => void;
}) {
  const { store, snapshot } = useClient();
  const { colors, scheme } = useTheme();
  const dialogs = useDialogs();
  const [space, setSpace] = useState(initialSpace);
  const [draft, setDraft] = useState<unknown>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const intent = useRef(installIntent()).current;
  const chats =
    snapshot?.conversations.filter((chat) => chat.members.some((person) => person.is_me)) ?? [];
  const target = chats.find((chat) => chat.id === space);
  const finish = useDirtyEditor(JSON.stringify(draft) !== '{}', busy);
  const creator = spec.tools.find(
    (tool) => tool.visibility.includes('model') && !tool.visibility.includes('app'),
  );
  if (!creator)
    return (
      <EmptyState
        title={t('miniapps.thisMiniappCannotBeInstalledYet')}
        detail={t('miniapps.theCatalogDidNotProvideACreationForm')}
      />
    );
  let schema;
  try {
    schema = parseSchema(creator.input_schema_json);
  } catch (reason) {
    return (
      <EmptyState
        title={t('miniapps.formUnavailable')}
        detail={reason instanceof Error ? reason.message : t('miniapps.thisFormCouldNotBeOpened')}
      />
    );
  }
  return (
    <>
      <Text
        accessibilityRole="header"
        style={{ color: colors.text, fontSize: 28, fontWeight: '600' }}
      >
        {spec.name}
      </Text>
      <Text style={{ color: colors.secondary, fontSize: 16, lineHeight: 24 }}>
        {spec.description}
      </Text>
      <Text style={{ color: colors.secondary, fontWeight: '600' }}>{t('miniapps.addToChat')}</Text>
      <Host
        matchContents={{ vertical: true }}
        style={{ width: '100%' }}
        colorScheme={scheme}
        seedColor={colors.accent}
      >
        <Picker
          testID="miniapp-install-conversation"
          appearance="menu"
          selectedValue={target?.id ?? ''}
          onValueChange={setSpace}
          enabled={!busy && chats.length > 0}
        >
          <Picker.Item label={t('miniapps.chooseChat')} value="" />
          {chats.map((chat) => (
            <Picker.Item key={chat.id} label={chat.title} value={chat.id} />
          ))}
        </Picker>
      </Host>
      {chats.length === 0 ? (
        <Text style={{ color: colors.secondary }}>{t('miniapps.createChatToAddMiniapp')}</Text>
      ) : null}
      <SchemaForm schema={schema} value={draft} onChange={setDraft} disabled={busy} />
      {error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error}
        </Text>
      ) : null}
      <NativeButton
        label={busy ? t('miniapps.adding') : t('miniapps.addMiniapp')}
        disabled={busy || !target}
        onPress={() => {
          if (!target) return;
          setBusy(true);
          setError(null);
          void (async () => {
            let args_json = exactJson(formValue(schema, draft)!);
            if (isBinaryEditorId(spec.id)) {
              const epoch = store.sessionEpoch;
              const key = JSON.stringify([epoch, target.id, spec.id, args_json]);
              const mutationId = await intent(key, () => store.request({ op: 'new_mutation_id' }));
              if (store.sessionEpoch !== epoch)
                throw Error(t(`${binaryEditorFormat(spec.id).messages}.accountChanged`));
              args_json = exactJson({ ...JSON.parse(args_json), mutationId });
            }
            const item = await store.request({
              op: 'install_app',
              space_id: target.id,
              app_id: spec.id,
              args_json,
            });
            finish();
            router.replace({ pathname: '/miniapp/[id]', params: { id: item.id } });
          })()
            .catch((reason: unknown) =>
              setError(
                reason instanceof Error
                  ? reason.message
                  : t('miniapps.thisMiniappCouldNotBeInstalled'),
              ),
            )
            .finally(() => setBusy(false));
        }}
      />
      <NativeButton
        secondary
        label={t('miniapps.backToCatalog')}
        disabled={busy}
        onPress={() => {
          void (async () => {
            if (
              JSON.stringify(draft) !== '{}' &&
              !(await dialogs.confirm({
                title: t('miniapps.discardThisForm'),
                message: t('miniapps.theMiniappHasNotBeenCreatedYetYour'),
                confirmLabel: t('miniapps.discard'),
                destructive: true,
              }))
            )
              return;
            finish();
            onBack();
          })();
        }}
      />
    </>
  );
}
export default function Miniapps() {
  const { space: initialSpace } = useLocalSearchParams<{ space?: string }>();
  const { colors } = useTheme();
  const read = useCoreQuery({ op: 'app_specs' });
  const [selected, setSelected] = useState<AppSpecDto | null>(null);
  return (
    <ScrollView
      key={selected?.id ?? 'catalog'}
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{
        padding: 24,
        paddingBottom: 48,
        gap: 20,
        maxWidth: 850,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen options={{ title: t('miniapps.miniapps') }} />
      {selected ? (
        <Install
          key={selected.id}
          spec={selected}
          initialSpace={initialSpace ?? ''}
          onBack={() => setSelected(null)}
        />
      ) : (
        <>
          <Text
            accessibilityRole="header"
            style={{ color: colors.text, fontSize: 32, fontWeight: '600' }}
          >
            {t('miniapps.littleThingsTogether')}
          </Text>
          <Text style={{ color: colors.secondary, fontSize: 16, lineHeight: 24 }}>
            {t('miniapps.listsPollsRecipesAndSchedulesSharedInYour')}
          </Text>
          <NativeButton
            secondary
            label={t('miniapps.miniappPermissions')}
            onPress={() => router.push('/miniapp-permissions')}
          />
          {!read.data ? (
            read.error ? (
              <EmptyState title={t('miniapps.theCatalogCouldNotBeOpened')} detail={read.error}>
                <NativeButton label={t('miniapps.tryAgain')} onPress={read.retry} />
              </EmptyState>
            ) : (
              <Loading />
            )
          ) : (
            read.data.map((spec) => (
              <Surface key={spec.id}>
                <Text style={{ color: colors.text, fontSize: 23, fontWeight: '600' }}>
                  {spec.name}
                </Text>
                <Text style={{ color: colors.secondary, fontSize: 15, lineHeight: 23 }}>
                  {spec.description}
                </Text>
                <NativeButton
                  secondary
                  label={t('miniapps.addNamedApp', { name: spec.name })}
                  onPress={() => setSelected(spec)}
                />
              </Surface>
            ))
          )}
        </>
      )}
    </ScrollView>
  );
}
