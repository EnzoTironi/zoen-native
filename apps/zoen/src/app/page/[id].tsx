import { t } from '../../i18n/index.ts';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Stack, router, useLocalSearchParams, useFocusEffect } from 'expo-router';
import { AppState, ScrollView, Text, View } from 'react-native';
import { useClient } from '../../core/provider';
import { useCoreQuery } from '../../core/query';
import type { Command, PageDto } from '../../core/types';
import type { PageBlockDto } from '../../core/generated/PageBlockDto';
import { EmptyState, Loading, NativeButton, Surface } from '../../components/primitives';
import { useDialogs } from '../../components/dialogs';
import { ArtifactDiscussion } from '../../documents/artifact-discussion';
import { PageBlocks } from '../../documents/page-blocks';
import { duplicateBlockChange } from '../../documents/page-duplicate';
import { documentScope } from '../../documents/document-authority';
import { homeSpacesFor } from '../../home/access';
import { moveBlock } from '../../documents/editing';
import { exportMarkdown } from '../../documents/markdown-files';
import { useDirtyEditor } from '../../documents/use-dirty-editor';
import { useTheme } from '../../theme';

type Apply = Extract<Command, { op: 'page_apply' }>;
type Undo = { context: string; order: string[]; changed: PageBlockDto[]; label: string };

export default function Page() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const client = useClient();
  const owner = documentScope(client, client.store.sessionEpoch);
  return <PageDocument key={JSON.stringify([owner, id])} id={id} />;
}
function PageDocument({ id }: { id: string }) {
  const client = useClient();
  const { store } = client;
  const ownerScope = documentScope(client, store.sessionEpoch);
  const { data: page, error, retry } = useCoreQuery({ op: 'page', item_id: id });
  const dialogs = useDialogs();
  const { colors } = useTheme();
  const [busy, setBusy] = useState(false);
  const [organizing, setOrganizing] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const [undo, setUndo] = useState<Undo | null>(null);
  const pending = useRef<{ command: Apply; inverse: Omit<Undo, 'context'> } | null>(null);
  const working = useRef(false);
  const focus = useRef(false);
  const activation = useRef<object | null>(null);
  const latest = useRef<PageDto | null>(null);
  useLayoutEffect(() => {
    latest.current = page;
  }, [page]);
  useFocusEffect(
    useCallback(() => {
      focus.current = true;
      activation.current = AppState.currentState === 'active' ? {} : null;
      setBusy(false);
      return () => {
        focus.current = false;
        activation.current = null;
      };
    }, []),
  );
  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => {
      activation.current = state === 'active' && focus.current ? {} : null;
      if (state === 'active' && focus.current) setBusy(false);
    });
    return () => {
      focus.current = false;
      activation.current = null;
      subscription.remove();
    };
  }, []);
  useDirtyEditor(!!pending.current, busy);
  if (!page)
    return error ? (
      <EmptyState title={t('documents.thisPageCouldNotBeOpened')} detail={error}>
        <NativeButton label={t('documents.tryAgain')} onPress={retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  const observed = page;
  const durableInput = !!store.driver.pageInput && store.getSnapshot().mode === 'account';
  const pendingInput = durableInput && page.input_pending;

  function currentAction(editing = true) {
    const scope = ownerScope;
    const lease = activation.current;
    const driver = store.driver;
    return () => {
      const live = latest.current;
      return Boolean(
        lease &&
        lease === activation.current &&
        focus.current &&
        scope &&
        documentScope(store.getSnapshot(), store.sessionEpoch) === scope &&
        store.driver === driver &&
        !store.isSessionRetiring &&
        store.getSnapshot().phase === 'ready' &&
        homeSpacesFor(store.getSnapshot()).has(observed.space_id) &&
        live?.item_id === id &&
        live.space_id === observed.space_id &&
        live.ready &&
        (!editing || live.can_edit) &&
        !(store.driver.pageInput && store.getSnapshot().mode === 'account' && live.input_pending),
      );
    };
  }
  async function apply(
    order: string[],
    changed: PageBlockDto[],
    label: string,
    build?: (id: string) => { order: string[]; changed: PageBlockDto[] },
  ) {
    const current = currentAction();
    const settled = currentAction(false);
    if (working.current || busy || pending.current || pendingInput || !current()) return;
    working.current = true;
    setBusy(true);
    setFailure(null);
    const before = observed.blocks.map((block) => ({
      ...block,
      spans: block.spans.map((span) => ({ ...span })),
    }));
    const edit = {
      order: [...order],
      changed: changed.map((block) => ({
        ...block,
        spans: block.spans.map((span) => ({ ...span })),
      })),
    };
    try {
      const mutation_id = await store.driver.request({ op: 'new_mutation_id' });
      if (!current()) return;
      const desired = build ? build(mutation_id) : edit;
      pending.current = {
        command: {
          op: 'page_apply',
          item_id: id,
          mutation_id,
          edit_context: observed.edit_context,
          ...desired,
        },
        inverse: {
          order: before.map((block) => block.id),
          changed: before.filter(
            (block) =>
              desired.changed.some((next) => next.id === block.id) ||
              !desired.order.includes(block.id),
          ),
          label,
        },
      };
      await submitPending(current);
    } catch (reason) {
      if (current())
        setFailure(
          reason instanceof Error ? reason.message : t('documents.theDraftCouldNotBeUpdated'),
        );
    } finally {
      working.current = false;
      if (settled()) setBusy(false);
    }
  }
  async function submitPending(current = currentAction()) {
    const request = pending.current;
    if (!request || !current()) return;
    const result = await store.request(request.command);
    if (!current()) return;
    setUndo({ ...request.inverse, context: result.applied_edit_context });
    pending.current = null;
    retry();
  }
  async function undoEdit() {
    const current = currentAction();
    const settled = currentAction(false);
    if (!undo || working.current || busy || pending.current || !current()) return;
    const previous = undo;
    working.current = true;
    setBusy(true);
    setFailure(null);
    try {
      const mutation_id = await store.driver.request({ op: 'new_mutation_id' });
      if (!current()) return;
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
          label: t('documents.undo'),
        },
      };
      await submitPending(current);
      if (current()) setUndo(null);
    } catch (reason) {
      if (current())
        setFailure(reason instanceof Error ? reason.message : t('documents.thisCouldNotBeUndone'));
    } finally {
      working.current = false;
      if (settled()) setBusy(false);
    }
  }
  async function run(
    action: (current: () => boolean) => Promise<unknown>,
    message: string,
    editing = true,
  ) {
    const current = currentAction(editing);
    const settled = currentAction(false);
    if (working.current || busy || !current()) return;
    working.current = true;
    setBusy(true);
    setFailure(null);
    try {
      await action(current);
      if (current()) retry();
    } catch (reason) {
      if (current()) setFailure(reason instanceof Error ? reason.message : message);
    } finally {
      working.current = false;
      if (settled()) setBusy(false);
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
          title: t('documents.page'),
          headerRight: () => (
            <NativeButton
              secondary
              label={t('documents.history')}
              onPress={() => router.push({ pathname: '/item/[id]', params: { id, history: '1' } })}
            />
          ),
        }}
      />
      <View style={{ gap: 6, marginBottom: 14 }}>
        <Text style={{ color: colors.accent, fontWeight: '600', fontSize: 12 }}>
          {t('documents.pageVersion', { version: page.version })}
        </Text>
        <Text
          accessibilityLiveRegion="polite"
          style={{ color: page.save_error ? colors.danger : colors.secondary, fontSize: 13 }}
        >
          {page.save_error ??
            (pendingInput
              ? t('documents.editorInputRetained')
              : page.unsaved
                ? t('documents.draftSavedOnThisDevice')
                : page.pending_sync
                  ? t('documents.waitingToSync')
                  : t('documents.sharedInChat'))}
        </Text>
      </View>
      <ArtifactDiscussion itemId={id} spaceId={page.space_id} />
      {failure || error || pending.current ? (
        <Surface>
          {failure || error ? (
            <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
              {failure ?? error}
            </Text>
          ) : null}
          {pending.current ? (
            <NativeButton
              label={t('documents.retryTheSameChange')}
              disabled={busy}
              onPress={() => {
                void run(submitPending, t('documents.thisCouldNotBeSaved'));
              }}
            />
          ) : null}
        </Surface>
      ) : null}
      {pendingInput ? (
        <Surface>
          <Text style={{ color: colors.secondary }}>{t('documents.editorInputRetained')}</Text>
          <NativeButton
            label={t('documents.recoverEditorInput')}
            disabled={busy}
            onPress={() =>
              router.push({
                pathname: '/page-block/[id]',
                params: { id, block: 'new', recover: '1' },
              })
            }
          />
        </Surface>
      ) : null}
      {undo ? (
        <Surface>
          <Text style={{ color: colors.secondary }}>
            {t('documents.draftSavedMessage', { message: undo.label })}
          </Text>
          <NativeButton
            secondary
            label={t('documents.undoLastChange')}
            disabled={busy || !!pending.current || pendingInput}
            onPress={() => {
              void undoEdit();
            }}
          />
        </Surface>
      ) : null}
      {!page.ready ? (
        <EmptyState
          title={t('documents.thisPageIsOnItsWay')}
          detail={t('documents.theContentWillAppearWhenThisDeviceFinishes')}
        />
      ) : (
        <PageBlocks
          page={page}
          blocks={page.blocks}
          editable={page.can_edit && !pending.current && !pendingInput}
          busy={busy}
          organizing={organizing}
          onEdit={(block) =>
            router.push({ pathname: '/page-block/[id]', params: { id, block: block.id } })
          }
          onToggle={(block) => {
            void apply(
              observed.blocks.map((item) => item.id),
              [{ ...block, checked: !block.checked }],
              t('documents.taskUpdated'),
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
              t('documents.orderUpdated'),
            );
          }}
          onDuplicate={(block) => {
            const build = duplicateBlockChange(
              observed.blocks.map((item) => item.id),
              block,
            );
            void apply([], [], t('documents.blockDuplicated'), build);
          }}
          onDelete={(block) => {
            const current = currentAction();
            if (!current()) return;
            void dialogs
              .confirm({
                title: t('documents.deleteThisBlock'),
                message: block.text || t('documents.theBlockWillBeRemovedFromYourDraft'),
                confirmLabel: t('documents.deleteBlock'),
                destructive: true,
              })
              .then((accepted) => {
                if (accepted && current())
                  void apply(
                    observed.blocks.filter((item) => item.id !== block.id).map((item) => item.id),
                    [],
                    t('documents.blockDeleted'),
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
              label={t('documents.addABlock')}
              disabled={busy || !!pending.current || pendingInput}
              onPress={() =>
                router.push({ pathname: '/page-block/[id]', params: { id, block: 'new' } })
              }
            />
            {durableInput ? (
              <NativeButton
                secondary
                label={t('documents.editMarkdown')}
                disabled={busy || !!pending.current || pendingInput}
                onPress={() =>
                  router.push({
                    pathname: '/page-block/[id]',
                    params: { id, block: 'new', mode: 'markdown' },
                  })
                }
              />
            ) : null}
            <NativeButton
              secondary
              label={organizing ? t('documents.finishOrganizing') : t('documents.organizeBlocks')}
              disabled={busy || !!pending.current || pendingInput}
              onPress={() => setOrganizing((value) => !value)}
            />
          </View>
          <NativeButton
            label={busy ? t('documents.saving') : t('documents.shareChanges')}
            disabled={busy || !page.unsaved || !!pending.current || pendingInput}
            onPress={() => {
              void run(async () => {
                await store.request({
                  op: 'page_commit',
                  item_id: id,
                  note: t('documents.pageUpdated'),
                });
                setUndo(null);
              }, t('documents.theChangesCouldNotBeShared'));
            }}
          />
        </View>
      ) : null}
      {page.ready ? (
        <NativeButton
          secondary
          label={t('documents.exportMarkdown')}
          disabled={busy || pendingInput}
          onPress={() => {
            void run(
              async (current) => {
                const markdown = await store.driver.request({ op: 'page_markdown', item_id: id });
                if (current()) await exportMarkdown(page.title, markdown, current);
              },
              t('documents.thePageCouldNotBeExported'),
              false,
            );
          }}
        />
      ) : null}
    </ScrollView>
  );
}
