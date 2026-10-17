import { useState } from 'react';
import { Redirect, Stack, router, useLocalSearchParams } from 'expo-router';
import { ScrollView, Text, View, Pressable } from 'react-native';
import { useClient } from '../../core/provider';
import { useCoreQuery } from '../../core/query';
import { useTheme } from '../../theme';
import { EmptyState, Loading, NativeButton, Surface } from '../../components/primitives';
import { useDialogs } from '../../components/dialogs';
import { PlanBlocks, PlanVersion } from '../../documents/plan-blocks';
import { PageBlocks } from '../../documents/page-blocks';
import { clearPlanUndo, rememberPlanUndo, usePlanUndo } from '../../documents/plan-undo';
import { FileAttachment } from '../../media/MediaAttachment';
import { MiniappState } from '../../miniapps/state-view';

export default function Item() {
  const { id, history } = useLocalSearchParams<{ id: string; history?: string }>();
  const { store } = useClient();
  const { colors } = useTheme();
  const dialogs = useDialogs();
  const [selected, setSelected] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const itemRead = useCoreQuery({ op: 'item', item_id: id });
  const item = itemRead.data;
  const planRead = useCoreQuery(item?.kind_id === 'plan' ? { op: 'plan_page', item_id: id } : null);
  const pageRead = useCoreQuery(item?.kind_id === 'page' ? { op: 'page', item_id: id } : null);
  const past = useCoreQuery(
    selected !== null && item ? { op: 'item_at', item_id: id, version: selected } : null,
  );
  const pastPage = useCoreQuery(
    selected !== null && item?.kind_id === 'page'
      ? { op: 'page_at', item_id: id, version: selected }
      : null,
  );
  const undo = usePlanUndo(store, id);
  if (!item)
    return itemRead.error ? (
      <EmptyState title="Não foi possível abrir este item" detail={itemRead.error}>
        <NativeButton label="Tentar novamente" onPress={itemRead.retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  if (item.kind_id === 'page' && !history)
    return <Redirect href={{ pathname: '/page/[id]', params: { id } }} />;
  if (item.app && !history)
    return <Redirect href={{ pathname: '/miniapp/[id]', params: { id } }} />;
  const observedItem = item;
  const plan = planRead.data;
  const page = pageRead.data;
  const readError = itemRead.error ?? planRead.error ?? pageRead.error;
  const oldError = past.error ?? pastPage.error;
  const oldReady = past.data && (item.kind_id !== 'page' || pastPage.data);
  const canRestore =
    selected !== null &&
    selected !== item.version &&
    (item.kind_id === 'plan'
      ? !!plan?.ready && plan.legacy_edit_admitted
      : item.kind_id === 'page'
        ? !!page?.ready && page.can_edit
        : ['note', 'task', 'app', 'file'].includes(item.kind_id));
  async function run(action: () => Promise<unknown>) {
    if (busy) return;
    setBusy(true);
    setFailure(null);
    try {
      await action();
      itemRead.retry();
      planRead.retry();
      pageRead.retry();
    } catch (reason) {
      setFailure(
        reason instanceof Error ? reason.message : 'Não foi possível atualizar este item.',
      );
    } finally {
      setBusy(false);
    }
  }
  async function restore() {
    if (!canRestore || selected === null || busy) return;
    const version = selected;
    const accepted = await dialogs.confirm({
      title: `Restaurar a versão ${version}?`,
      message: page?.unsaved
        ? 'O rascunho atual será substituído. A restauração cria uma nova versão compartilhada; as versões anteriores continuam no histórico.'
        : 'A restauração cria uma nova versão compartilhada. As versões anteriores continuam no histórico.',
      confirmLabel: 'Restaurar versão',
      destructive: !!page?.unsaved,
    });
    if (!accepted) return;
    await run(async () => {
      if (observedItem.kind_id === 'plan' && plan)
        await store.request({
          op: 'restore_plan_version',
          item_id: id,
          version,
          observed_hash: plan.content_hash,
        });
      else await store.request({ op: 'restore_version', item_id: id, version });
      clearPlanUndo(store, id);
      setSelected(null);
    });
  }
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        padding: 24,
        paddingBottom: 48,
        gap: 18,
        maxWidth: 820,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen options={{ title: history ? 'Histórico' : item.kind_label }} />
      <Text style={{ color: colors.accent, fontSize: 13, fontWeight: '600' }}>
        {item.space_title}
      </Text>
      <Text
        accessibilityRole="header"
        selectable
        style={{
          color: colors.text,
          fontSize: 34,
          lineHeight: 40,
          fontWeight: '600',
          letterSpacing: -0.8,
        }}
      >
        {selected === null ? item.title : (past.data?.title ?? item.title)}
      </Text>
      {failure || readError ? (
        <Surface>
          <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
            {failure ?? readError}
          </Text>
          <NativeButton
            secondary
            label="Atualizar"
            disabled={busy}
            onPress={() => {
              itemRead.retry();
              planRead.retry();
              pageRead.retry();
            }}
          />
        </Surface>
      ) : null}
      {selected !== null ? (
        <>
          <Surface>
            <Text
              accessibilityLiveRegion="polite"
              style={{ color: colors.accent, fontWeight: '600' }}
            >
              PRÉVIA · VERSÃO {selected}
            </Text>
            <Text style={{ color: colors.secondary }}>
              Esta versão está disponível somente para leitura.
            </Text>
            <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 12 }}>
              <NativeButton
                secondary
                label="Voltar à versão atual"
                disabled={busy}
                onPress={() => setSelected(null)}
              />
              {canRestore && oldReady ? (
                <NativeButton
                  label={busy ? 'Restaurando…' : 'Restaurar esta versão'}
                  disabled={busy}
                  onPress={() => {
                    void restore();
                  }}
                />
              ) : null}
            </View>
          </Surface>
          {oldError ? (
            <EmptyState title="Não foi possível carregar esta versão" detail={oldError}>
              <NativeButton
                label="Tentar novamente"
                onPress={() => {
                  past.retry();
                  pastPage.retry();
                }}
              />
            </EmptyState>
          ) : !oldReady ? (
            <Loading />
          ) : pastPage.data ? (
            <PageBlocks blocks={pastPage.data.blocks} />
          ) : past.data?.plan ? (
            <PlanVersion plan={past.data.plan} />
          ) : past.data?.file ? (
            <FileAttachment
              itemId={id}
              version={selected}
              title={past.data.title}
              expectedHash={past.data.file.sha256}
            />
          ) : past.data?.app ? (
            <MiniappState app={past.data.app} busy onCall={() => {}} />
          ) : (
            <Text selectable style={{ color: colors.text, fontSize: 17, lineHeight: 27 }}>
              {past.data?.text ?? 'Esta versão não possui conteúdo de texto.'}
            </Text>
          )}
        </>
      ) : (
        <>
          {undo &&
          plan &&
          undo.result_version === plan.version &&
          undo.observed_hash === plan.content_hash ? (
            <Surface>
              <Text style={{ color: colors.secondary }}>{undo.label}</Text>
              <NativeButton
                secondary
                label="Desfazer última alteração"
                disabled={busy}
                onPress={() => {
                  void run(async () => {
                    await store.request({ op: 'undo', token: undo });
                    clearPlanUndo(store, id);
                  });
                }}
              />
            </Surface>
          ) : null}
          {item.kind_id === 'plan' ? (
            plan ? (
              <PlanBlocks
                plan={plan}
                busy={busy}
                onToggle={(block) => {
                  if (typeof block.content !== 'object' || !('Line' in block.content)) return;
                  const line = block.content.Line;
                  void run(async () => {
                    const result = await store.request({
                      op: 'toggle_plan_line',
                      item_id: id,
                      line_id: line.legacy_id,
                      observed_hash: plan.content_hash,
                    });
                    rememberPlanUndo(store, result.undo);
                  });
                }}
                onEdit={(block) => {
                  if (typeof block.content === 'object' && 'Line' in block.content)
                    router.push({
                      pathname: '/item/[id]/line',
                      params: { id, line: block.content.Line.legacy_id },
                    });
                }}
                onAdd={(section) =>
                  router.push({
                    pathname: '/item/[id]/line',
                    params: { id, section: String(section) },
                  })
                }
              />
            ) : planRead.error ? null : (
              <Loading />
            )
          ) : item.kind_id === 'page' ? (
            page ? (
              <>
                <PageBlocks blocks={page.blocks} />
                <NativeButton
                  secondary
                  label="Abrir página"
                  onPress={() => router.push({ pathname: '/page/[id]', params: { id } })}
                />
              </>
            ) : pageRead.error ? null : (
              <Loading />
            )
          ) : item.app ? (
            <>
              <MiniappState app={item.app} busy onCall={() => {}} />
              <NativeButton
                label="Abrir miniapp"
                onPress={() => router.push({ pathname: '/miniapp/[id]', params: { id } })}
              />
            </>
          ) : item.file ? (
            <FileAttachment
              itemId={id}
              version={item.version}
              title={item.title}
              expectedHash={item.file.sha256}
            />
          ) : item.text ? (
            <Text selectable style={{ color: colors.text, fontSize: 17, lineHeight: 27 }}>
              {item.text}
            </Text>
          ) : (
            <Text style={{ color: colors.secondary }}>{item.kind_label}</Text>
          )}
        </>
      )}
      <View
        style={{
          marginTop: 24,
          paddingTop: 18,
          gap: 10,
          borderTopWidth: 1,
          borderTopColor: colors.line,
        }}
      >
        <Text accessibilityRole="header" style={{ color: colors.secondary, fontWeight: '600' }}>
          Histórico · versão atual {item.version}
        </Text>
        {item.versions.map((version) => (
          <Pressable
            key={version.number}
            disabled={busy}
            accessibilityRole="button"
            accessibilityLabel={`Prévia da versão ${version.number}: ${version.note}`}
            accessibilityState={{ selected: selected === version.number, disabled: busy }}
            aria-pressed={process.env.EXPO_OS === 'web' ? selected === version.number : undefined}
            onPress={() => {
              setFailure(null);
              setSelected(version.number);
            }}
            style={{
              flexDirection: 'row',
              alignItems: 'flex-start',
              gap: 12,
              padding: 14,
              borderRadius: 16,
              backgroundColor: selected === version.number ? colors.muted : colors.surface,
            }}
          >
            <Text style={{ color: colors.accent, fontWeight: '600' }}>v{version.number}</Text>
            <View style={{ flex: 1, gap: 5 }}>
              <Text style={{ color: colors.text }}>{version.note}</Text>
              <Text style={{ color: colors.secondary, fontSize: 12 }}>
                {version.author.name} · {new Date(Number(version.at_ms)).toLocaleString('pt-BR')}
              </Text>
            </View>
            <Text style={{ color: colors.tertiary }}>›</Text>
          </Pressable>
        ))}
      </View>
    </ScrollView>
  );
}
