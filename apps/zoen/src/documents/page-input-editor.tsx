import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { Stack, router, usePreventRemove } from 'expo-router';
import { ScrollView, Text, View } from 'react-native';
import type { PageDto } from '../core/types';
import type { PageInputMode, PageInputPort, PageInputTarget } from './page-input';
import { PageInputConflict, PageInputEditor } from './page-input';
import { PageInputView } from './page-input-view';
import { BlockEditor } from '../components/block-editor';
import { Loading, NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import { useDialogs } from '../components/dialogs';
import { t } from '../i18n';
import { useTheme } from '../theme';

export function DurablePageEditor({
  page,
  blockId,
  port,
  mode,
  resume,
  target,
}: {
  page: PageDto;
  blockId: string;
  port: PageInputPort;
  mode?: PageInputMode;
  resume: boolean;
  target?: PageInputTarget;
}) {
  const [session, setSession] = useState<{
    page: PageDto;
    mode: PageInputMode | undefined;
    serial: number;
  }>({ page, mode, serial: 0 });
  return (
    <Editor
      key={session.serial}
      page={session.page}
      blockId={blockId}
      port={port}
      mode={session.mode}
      resume={resume}
      target={target}
      onMode={(nextPage, nextMode) => {
        setSession((value) => ({ page: nextPage, mode: nextMode, serial: value.serial + 1 }));
      }}
    />
  );
}

function Editor({
  page,
  blockId,
  port,
  mode,
  resume,
  target,
  onMode,
}: {
  page: PageDto;
  blockId: string;
  port: PageInputPort;
  mode?: PageInputMode;
  resume: boolean;
  target?: PageInputTarget;
  onMode(page: PageDto, mode: PageInputMode): void;
}) {
  const { store } = useClient();
  const { colors } = useTheme();
  const dialogs = useDialogs();
  const original =
    target === 'document' ? undefined : page.blocks.find((block) => block.id === blockId);
  const canRich =
    !original ||
    (['paragraph', 'heading', 'bullet', 'numbered'].includes(original.kind) &&
      original.spans.every((span) => ['b', 'i', 's', 'a'].includes(span.key)));
  const selectedMode =
    (target === 'document' ? 'markdown' : mode) ??
    (original &&
    (!['paragraph', 'heading', 'bullet', 'numbered'].includes(original.kind) ||
      original.spans.some((span) => !['b', 'i', 's', 'a'].includes(span.key)))
      ? 'lossless-text'
      : 'rich');
  const [editor] = useState(
    () =>
      new PageInputEditor(port, {
        itemId: page.item_id,
        blockId: target === 'document' || blockId === 'new' ? null : blockId,
        target,
        editContext: page.edit_context,
        mode: selectedMode,
        resume,
      }),
  );
  const status = useSyncExternalStore(editor.subscribe, editor.getSnapshot, editor.getSnapshot);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [leaving, setLeaving] = useState(false);
  const navigating = useRef(false);
  const working = useRef(false);
  const active = useRef(true);

  const disableGuard = usePreventRemove(!leaving, ({ repeat }) => {
    if (navigating.current || working.current) return;
    navigating.current = true;
    setBusy(true);
    void editor
      .close()
      .then(() => {
        setLeaving(true);
        repeat();
      })
      .catch((reason: unknown) => {
        if (active.current)
          setError(
            reason instanceof Error ? reason.message : t('documents.editorInputNotRetained'),
          );
      })
      .finally(() => {
        navigating.current = false;
        if (active.current) setBusy(false);
      });
  });

  const reportOpenError = useCallback((reason: unknown) => {
    if (!active.current) return;
    setConflict(reason instanceof PageInputConflict);
    setError(
      reason instanceof PageInputConflict
        ? t('documents.editorInputConflict')
        : reason instanceof Error
          ? reason.message
          : t('documents.theEditorCouldNotBeOpened'),
    );
  }, []);
  const open = useCallback(async () => {
    try {
      await editor.open();
      if (active.current) {
        setError(null);
        setConflict(false);
      }
    } catch (reason) {
      reportOpenError(reason);
    }
  }, [editor, reportOpenError]);
  useEffect(() => {
    active.current = true;
    void editor.open().catch(reportOpenError);
    const unregister = store.registerSessionCleanup((discard) => {
      if (discard) {
        editor.retire();
        return Promise.resolve();
      }
      return editor.close();
    });
    return () => {
      active.current = false;
      unregister();
      // Normal route removal has already awaited close. Forced JS teardown may
      // only detach; the native session still owns and fences its producers.
      void editor.close().catch(() => {});
    };
  }, [editor, reportOpenError, store]);

  async function run(work: () => Promise<void>) {
    if (working.current) return;
    working.current = true;
    setBusy(true);
    setError(null);
    try {
      await work();
    } catch (reason) {
      if (active.current)
        setError(
          reason instanceof Error ? reason.message : t('documents.thisDraftCouldNotBeSaved'),
        );
    } finally {
      working.current = false;
      if (active.current) setBusy(false);
    }
  }
  async function leave() {
    await editor.close();
    disableGuard();
    setLeaving(true);
    router.back();
  }
  async function discard() {
    if (
      !(await dialogs.confirm({
        title: t('documents.discardEditorInput'),
        message: t('documents.editorInputRestored'),
        confirmLabel: t('documents.discardChanges'),
        cancelLabel: t('documents.keepEditing'),
        destructive: true,
      }))
    )
      return;
    await editor.discard();
    await leave();
  }
  async function switchMode(nextMode: PageInputMode) {
    // Changing representation consumes the exact native capture first. A lost
    // Apply response stays retryable; it can never seed a second editor from JS text.
    if (
      !(await dialogs.confirm({
        title: t('documents.applyEditorInput'),
        message: t('documents.changesStayInYourDraftShareThePage'),
        confirmLabel: t('documents.saveDraft'),
        cancelLabel: t('documents.keepEditing'),
      }))
    )
      return;
    await editor.apply();
    await editor.close();
    const current = await store.driver.request({ op: 'page', item_id: page.item_id });
    disableGuard();
    onMode(current, nextMode);
  }
  const failed = !!status?.failed || !!status?.retentionPending;
  const pendingOperation = status?.operation;
  return (
    <ScrollView
      style={{ flex: 1 }}
      contentInsetAdjustmentBehavior="automatic"
      automaticallyAdjustKeyboardInsets
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{
        padding: 24,
        gap: 18,
        maxWidth: 820,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen
        options={{
          title:
            (status?.target ?? target) === 'document'
              ? t('documents.editMarkdown')
              : original
                ? t('documents.editBlock')
                : t('documents.newBlock'),
        }}
      />
      {status?.token ? (
        <>
          {status.mode === 'rich' ? (
            <BlockEditor
              initial={status.initialMarkdown ?? ''}
              checkpointToken={status.token}
              disabled={busy || failed || status.sealed || status.completed}
            />
          ) : (
            <PageInputView token={status.token} style={{ height: 330, width: '100%' }} />
          )}
          <Text
            accessibilityLiveRegion="polite"
            style={{ color: failed ? colors.danger : colors.secondary, lineHeight: 22 }}
          >
            {failed
              ? t('documents.editorInputNotRetained')
              : status.hasRetainedCapture
                ? t('documents.editorInputRetained')
                : t('documents.changesStayInYourDraftShareThePage')}
          </Text>
          {status.resuming ? (
            <Text style={{ color: colors.secondary }}>{t('documents.editorInputRestored')}</Text>
          ) : null}
          {failed && !pendingOperation ? (
            <NativeButton
              label={t('documents.tryAgain')}
              disabled={busy}
              onPress={() => {
                void run(() => editor.retryRetention());
              }}
            />
          ) : null}
          <NativeButton
            label={
              pendingOperation === 'apply'
                ? t('documents.retryTheSameChange')
                : t('documents.applyEditorInput')
            }
            disabled={
              busy || (failed && pendingOperation !== 'apply') || pendingOperation === 'discard'
            }
            onPress={() => {
              void run(async () => {
                await editor.apply();
                await leave();
              });
            }}
          />
          <NativeButton
            secondary
            label={t('documents.discardEditorInput')}
            disabled={busy || pendingOperation === 'apply'}
            onPress={() => {
              void run(discard);
            }}
          />
          {original &&
          (status.mode !== 'markdown' || canRich) &&
          !pendingOperation &&
          !failed &&
          !status.completed ? (
            <NativeButton
              secondary
              label={
                status.mode === 'markdown'
                  ? t('documents.openVisualEditor')
                  : t('documents.editMarkdown')
              }
              disabled={busy}
              onPress={() => {
                void run(() => switchMode(status.mode === 'markdown' ? 'rich' : 'markdown'));
              }}
            />
          ) : null}
        </>
      ) : error ? null : (
        <Loading />
      )}
      {error ? (
        <View style={{ gap: 12 }}>
          <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
            {error}
          </Text>
          {!status ? (
            <NativeButton
              label={t('documents.tryAgain')}
              disabled={busy}
              onPress={() => {
                void open();
              }}
            />
          ) : null}
          {conflict ? (
            <NativeButton
              label={t('documents.recoverEditorInput')}
              disabled={busy}
              onPress={() => {
                void run(async () => {
                  await editor.close();
                  disableGuard();
                  setLeaving(true);
                  router.replace({
                    pathname: '/page-block/[id]',
                    params: { id: page.item_id, block: 'new', recover: '1' },
                  });
                });
              }}
            />
          ) : null}
        </View>
      ) : null}
      <NativeButton
        secondary
        label={t('documents.keepEditorInput')}
        disabled={busy}
        onPress={() => {
          void run(leave);
        }}
      />
    </ScrollView>
  );
}
