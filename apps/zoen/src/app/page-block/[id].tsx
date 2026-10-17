import { useRef, useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { ScrollView, Text, TextInput, View } from 'react-native';
import { useCoreQuery } from '../../core/query';
import { useClient } from '../../core/provider';
import type { Command, PageDto } from '../../core/types';
import { BlockEditor } from '../../components/block-editor';
import { EmptyState, Loading, NativeButton } from '../../components/primitives';
import { useDialogs } from '../../components/dialogs';
import { needsLosslessTextEditor, replaceBlock, retargetSpans } from '../../documents/editing';
import { useDirtyEditor } from '../../documents/use-dirty-editor';
import { useTheme } from '../../theme';

function Editor({ initialPage, blockId }: { initialPage: PageDto; blockId: string }) {
  const { store } = useClient();
  const { colors } = useTheme();
  // This is the observed CRDT context. Background sync cannot replace a live edit.
  const [page] = useState(initialPage);
  const original = page.blocks.find((block) => block.id === blockId);
  const lossless = !!original && needsLosslessTextEditor(original);
  const source = useCoreQuery(original ? { op: 'page_block_markdown', block: original } : null);
  const dialogs = useDialogs();
  const pending = useRef<Extract<Command, { op: 'page_apply' }> | null>(null);
  const [markdown, setMarkdown] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [losslessBlock, setLosslessBlock] = useState(original);
  const [sourceMode, setSourceMode] = useState(
    !!original &&
      (!['paragraph', 'heading', 'bullet', 'numbered'].includes(original.kind) ||
        original.spans.some((span) => !['b', 'i', 's', 'a'].includes(span.key))),
  );
  const dirty = lossless
    ? JSON.stringify(losslessBlock) !== JSON.stringify(original)
    : markdown !== null && markdown !== (source.data ?? '');
  const finish = useDirtyEditor(dirty, busy);
  if (!original && blockId !== 'new')
    return (
      <EmptyState title="Este bloco mudou" detail="Volte à página para abrir a versão atual." />
    );
  if (source.error && !lossless)
    return (
      <EmptyState title="Não foi possível abrir o editor" detail={source.error}>
        <NativeButton label="Tentar novamente" onPress={source.retry} />
      </EmptyState>
    );
  if (original && source.data === null && !lossless) return <Loading />;
  async function save() {
    if (busy || !page.can_edit) return;
    setBusy(true);
    setError(null);
    try {
      if (!pending.current) {
        const blocks =
          lossless && losslessBlock
            ? [losslessBlock]
            : await store.driver.request({
                op: 'page_parse_markdown',
                markdown: markdown ?? source.data ?? '',
              });
        if (
          original &&
          blocks.length === 0 &&
          !(await dialogs.confirm({
            title: 'Apagar este bloco?',
            message: 'O conteúdo está vazio. Guardar esta edição remove o bloco do rascunho.',
            confirmLabel: 'Apagar bloco',
            destructive: true,
          }))
        )
          return;
        const replacement = replaceBlock(page.blocks, original?.id ?? null, blocks);
        const mutation_id = await store.driver.request({ op: 'new_mutation_id' });
        pending.current = {
          op: 'page_apply',
          item_id: page.item_id,
          mutation_id,
          edit_context: page.edit_context,
          order: replacement.order,
          changed: replacement.changed,
        };
      }
      await store.request(pending.current);
      finish();
      router.back();
    } catch (failure) {
      setError(
        failure instanceof Error ? failure.message : 'Não foi possível guardar este rascunho.',
      );
    } finally {
      setBusy(false);
    }
  }
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{
        padding: 24,
        gap: 18,
        maxWidth: 820,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen options={{ title: original ? 'Editar bloco' : 'Novo bloco' }} />
      {lossless && losslessBlock ? (
        <View style={{ gap: 12 }}>
          <Text style={{ color: colors.secondary, lineHeight: 22 }}>
            Este bloco tem formatação adicional. O editor de texto preserva essas informações.
          </Text>
          <TextInput
            defaultValue={original?.text}
            multiline
            autoFocus
            editable={!busy && page.can_edit}
            accessibilityLabel="Texto do bloco, preservando formatação adicional"
            onChangeText={(value) => {
              setLosslessBlock((previous) =>
                previous
                  ? {
                      ...previous,
                      text: value,
                      spans: retargetSpans(previous.text, value, previous.spans),
                    }
                  : previous,
              );
              pending.current = null;
            }}
            style={{
              minHeight: 230,
              padding: 18,
              color: colors.text,
              backgroundColor: colors.surface,
              borderRadius: 18,
              fontSize: 17,
              lineHeight: 27,
              textAlignVertical: 'top',
            }}
          />
        </View>
      ) : (
        <>
          <BlockEditor
            key={sourceMode ? 'markdown' : 'rich'}
            initial={markdown ?? source.data ?? ''}
            sourceOnly={sourceMode}
            disabled={busy || !page.can_edit}
            onChange={(value) => {
              setMarkdown(value);
              pending.current = null;
            }}
          />
          {(!original ||
            (['paragraph', 'heading', 'bullet', 'numbered'].includes(original.kind) &&
              original.spans.every((span) => ['b', 'i', 's', 'a'].includes(span.key)))) &&
          process.env.EXPO_OS !== 'web' ? (
            <NativeButton
              secondary
              label={sourceMode ? 'Abrir editor visual' : 'Editar Markdown'}
              disabled={busy}
              onPress={() => setSourceMode((value) => !value)}
            />
          ) : null}
        </>
      )}
      {error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {error}
        </Text>
      ) : null}
      {!page.can_edit ? (
        <Text style={{ color: colors.secondary }}>
          Esta página está disponível somente para leitura.
        </Text>
      ) : (
        <NativeButton
          disabled={busy || !dirty}
          label={busy ? 'Guardando…' : 'Guardar rascunho'}
          onPress={() => {
            void save();
          }}
        />
      )}
      <Text style={{ color: colors.secondary, fontSize: 13, lineHeight: 20 }}>
        As mudanças ficam no seu rascunho. Compartilhe as alterações na página quando estiver
        pronto.
      </Text>
    </ScrollView>
  );
}
export default function PageBlock() {
  const { id, block } = useLocalSearchParams<{ id: string; block: string }>();
  const { data: page, error, retry } = useCoreQuery({ op: 'page', item_id: id });
  if (!page)
    return error ? (
      <EmptyState title="Não foi possível abrir a página" detail={error}>
        <NativeButton label="Tentar novamente" onPress={retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  return <Editor key={`${id}:${block}`} initialPage={page} blockId={block} />;
}
