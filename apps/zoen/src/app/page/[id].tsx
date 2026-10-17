import { useRef, useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { ScrollView, Text, View } from 'react-native';
import { useClient } from '../../core/provider';
import { useCoreQuery } from '../../core/query';
import type { Command } from '../../core/types';
import type { PageBlockDto } from '../../core/generated/PageBlockDto';
import { EmptyState, Loading, NativeButton, Surface } from '../../components/primitives';
import { useDialogs } from '../../components/dialogs';
import { PageBlocks } from '../../documents/page-blocks';
import { moveBlock } from '../../documents/editing';
import { exportMarkdown } from '../../documents/markdown-files';
import { useDirtyEditor } from '../../documents/use-dirty-editor';
import { useTheme } from '../../theme';

type Apply = Extract<Command, { op: 'page_apply' }>;
type Undo = { context: string; order: string[]; changed: PageBlockDto[]; label: string };

export default function Page() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { store } = useClient();
  const { data: page, error, retry } = useCoreQuery({ op: 'page', item_id: id });
  const dialogs = useDialogs();
  const { colors } = useTheme();
  const [busy, setBusy] = useState(false);
  const [organizing, setOrganizing] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [undo, setUndo] = useState<Undo | null>(null);
  const pending = useRef<{ command: Apply; inverse: Omit<Undo, 'context'> } | null>(null);
  useDirtyEditor(!!pending.current, busy);
  if (!page)
    return error ? (
      <EmptyState title="Não foi possível abrir esta página" detail={error}>
        <NativeButton label="Tentar novamente" onPress={retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  const observed = page;

  async function apply(order: string[], changed: PageBlockDto[], label: string) {
    if (busy || pending.current || !observed.can_edit) return;
    setBusy(true);
    setFailure(null);
    try {
      const mutation_id = await store.driver.request({ op: 'new_mutation_id' });
      pending.current = {
        command: {
          op: 'page_apply',
          item_id: id,
          mutation_id,
          edit_context: observed.edit_context,
          order,
          changed,
        },
        inverse: {
          order: observed.blocks.map((block) => block.id),
          changed: observed.blocks.filter(
            (block) => changed.some((next) => next.id === block.id) || !order.includes(block.id),
          ),
          label,
        },
      };
      await submitPending();
    } catch (reason) {
      setFailure(
        reason instanceof Error ? reason.message : 'Não foi possível atualizar o rascunho.',
      );
    } finally {
      setBusy(false);
    }
  }
  async function submitPending() {
    const request = pending.current;
    if (!request) return;
    const result = await store.request(request.command);
    setUndo({ ...request.inverse, context: result.applied_edit_context });
    pending.current = null;
    retry();
  }
  async function undoEdit() {
    if (!undo || busy) return;
    const previous = undo;
    setBusy(true);
    setFailure(null);
    try {
      const mutation_id = await store.driver.request({ op: 'new_mutation_id' });
      // Revert against the accepted edit, so unseen remote edits can merge.
      pending.current = {
        command: {
          op: 'page_apply',
          item_id: id,
          mutation_id,
          edit_context: previous.context,
          order: previous.order,
          changed: previous.changed,
        },
        inverse: {
          order: observed.blocks.map((block) => block.id),
          changed: observed.blocks,
          label: 'Desfazer',
        },
      };
      await submitPending();
      setUndo(null);
    } catch (reason) {
      setFailure(reason instanceof Error ? reason.message : 'Não foi possível desfazer.');
    } finally {
      setBusy(false);
    }
  }
  async function run(action: () => Promise<unknown>, message: string) {
    if (busy) return;
    setBusy(true);
    setFailure(null);
    try {
      await action();
      retry();
    } catch (reason) {
      setFailure(reason instanceof Error ? reason.message : message);
    } finally {
      setBusy(false);
    }
  }
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      contentContainerStyle={{
        padding: 26,
        paddingBottom: 48,
        gap: 18,
        maxWidth: 820,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen
        options={{
          title: 'Página',
          headerRight: () => (
            <NativeButton
              secondary
              label="Histórico"
              onPress={() => router.push({ pathname: '/item/[id]', params: { id, history: '1' } })}
            />
          ),
        }}
      />
      <View style={{ gap: 6, marginBottom: 14 }}>
        <Text style={{ color: colors.accent, fontWeight: '600', fontSize: 12 }}>
          PÁGINA · VERSÃO {page.version}
        </Text>
        <Text
          accessibilityLiveRegion="polite"
          style={{ color: page.save_error ? colors.danger : colors.secondary, fontSize: 13 }}
        >
          {page.save_error ??
            (page.unsaved
              ? 'Rascunho salvo neste aparelho'
              : page.pending_sync
                ? 'Aguardando sincronização'
                : 'Compartilhada na conversa')}
        </Text>
      </View>
      {failure || error ? (
        <Surface>
          <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
            {failure ?? error}
          </Text>
          {pending.current ? (
            <NativeButton
              label="Tentar a mesma alteração novamente"
              disabled={busy}
              onPress={() => {
                void run(submitPending, 'Não foi possível guardar.');
              }}
            />
          ) : null}
        </Surface>
      ) : null}
      {undo ? (
        <Surface>
          <Text style={{ color: colors.secondary }}>{undo.label} · rascunho guardado</Text>
          <NativeButton
            secondary
            label="Desfazer última alteração"
            disabled={busy || !!pending.current}
            onPress={() => {
              void undoEdit();
            }}
          />
        </Surface>
      ) : null}
      {!page.ready ? (
        <EmptyState
          title="Esta página está chegando"
          detail="O conteúdo aparecerá quando este aparelho terminar de sincronizar."
        />
      ) : (
        <PageBlocks
          blocks={page.blocks}
          editable={page.can_edit && !pending.current}
          busy={busy}
          organizing={organizing}
          onEdit={(block) =>
            router.push({ pathname: '/page-block/[id]', params: { id, block: block.id } })
          }
          onToggle={(block) => {
            void apply(
              observed.blocks.map((item) => item.id),
              [{ ...block, checked: !block.checked }],
              'Tarefa atualizada',
            );
          }}
          onMove={(block, direction) => {
            void apply(
              moveBlock(
                observed.blocks.map((item) => item.id),
                block.id,
                direction,
              ),
              [],
              'Ordem atualizada',
            );
          }}
          onDelete={(block) => {
            void dialogs
              .confirm({
                title: 'Apagar este bloco?',
                message: block.text || 'O bloco será removido do seu rascunho.',
                confirmLabel: 'Apagar bloco',
                destructive: true,
              })
              .then((accepted) => {
                if (accepted)
                  void apply(
                    observed.blocks.filter((item) => item.id !== block.id).map((item) => item.id),
                    [],
                    'Bloco apagado',
                  );
              });
          }}
        />
      )}
      {page.can_edit && page.ready ? (
        <View style={{ gap: 14, paddingTop: 18 }}>
          <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 12 }}>
            <NativeButton
              secondary
              label="Adicionar um bloco"
              disabled={busy || !!pending.current}
              onPress={() =>
                router.push({ pathname: '/page-block/[id]', params: { id, block: 'new' } })
              }
            />
            <NativeButton
              secondary
              label={organizing ? 'Concluir organização' : 'Organizar blocos'}
              disabled={busy || !!pending.current}
              onPress={() => setOrganizing((value) => !value)}
            />
          </View>
          <NativeButton
            label={busy ? 'Guardando…' : 'Compartilhar alterações'}
            disabled={busy || !page.unsaved || !!pending.current}
            onPress={() => {
              void run(async () => {
                await store.request({ op: 'page_commit', item_id: id, note: 'Página atualizada' });
                setUndo(null);
              }, 'Não foi possível compartilhar as alterações.');
            }}
          />
        </View>
      ) : null}
      {page.ready ? (
        <NativeButton
          secondary
          label="Exportar Markdown"
          disabled={busy}
          onPress={() => {
            void run(async () => {
              const markdown = await store.driver.request({ op: 'page_markdown', item_id: id });
              await exportMarkdown(page.title, markdown);
            }, 'Não foi possível exportar a página.');
          }}
        />
      ) : null}
    </ScrollView>
  );
}
