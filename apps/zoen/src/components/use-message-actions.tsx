import type { ReplySelection } from '../core/generated/ReplySelection';
import { messageInfoAction } from './message-info-navigation';
import { copyMessageLink } from './message-link-actions';
import { resolveMessageAuthor } from './message-action-current';
import { useMessageActionLifetime } from './use-message-action-lifetime';
import { canDeleteMessage } from './delete-message-action';
import { useMessageQuoteSelection } from './use-message-quote-selection';
import { useCallback, useState } from 'react';
import { router } from 'expo-router';
import { copyableMessageText, copyMessageText } from './message-copy';
import { KeyboardController } from 'react-native-keyboard-controller';
import type { TimelineEntry } from '../core/types';
import { useClient } from '../core/provider';
import {
  messageCanEdit,
  messageCanSelectText,
  messageCanSelect,
  messageCanRecoverMedia,
  messageCard,
} from '../core/message-presentation';
import { shareMessageMedia } from '../media/share-message';
import { t } from '../i18n/index.ts';
import { homeSpacesFor } from '../home/access';
import { useDialogs } from './dialogs';
import { MessageActions } from './message-actions';
import { ReactionDetails } from './reaction-details';
import type { MeasureMessageAnchor, MessageAnchor } from './message-actions.types';
import type { MessageContextRequest } from './message-context-ports';

export type { MeasureMessageAnchor, MessageAnchor } from './message-actions.types';
export type MessageActionMode = 'reactions' | 'actions' | 'participants' | 'emoji';

/** Chat and thread use the same canonical mutations and the same post-dismiss actions. */
export function useMessageActions({
  spaceId,
  entries,
  disabled = false,
  onReply,
  onEdit,
  onThread,
  onSelect,
  threadRootId = null,
}: {
  spaceId: string;
  entries: readonly TimelineEntry[];
  disabled?: boolean;
  threadRootId?: string | null;
  onReply: (entry: TimelineEntry, selection?: ReplySelection) => void;
  onEdit: (entry: TimelineEntry) => void;
  onThread?: (entry: TimelineEntry) => void;
  onSelect?: (entry: TimelineEntry) => void;
}) {
  const client = useClient();
  const { store, mode } = client;
  const { choose, inform } = useDialogs();
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<{ scope: string; id: string; text: string } | null>(null);
  const [selection, setSelection] = useState<{
    scope: string;
    id: string;
    mode: MessageActionMode;
    anchor?: MessageAnchor;
    emoji?: string;
    fromParticipants?: boolean;
    nativeSource?: MessageContextRequest;
  } | null>(null);
  const clearSelection = useCallback(() => setSelection(null), [setSelection]);
  const { scope, infoEpoch, controller, current } = useMessageActionLifetime({
    store,
    state: client,
    spaceId,
    entries,
    disabled,
    setBusy,
    clearSelection,
  });
  const quote = useMessageQuoteSelection(current, onReply, scope, spaceId, threadRootId);
  const candidate =
    !disabled && selection?.scope === scope
      ? (entries.find((entry) => entry.id === selection.id) ?? null)
      : null;
  const selected =
    homeSpacesFor(client).has(spaceId) &&
    candidate &&
    messageCanSelect(candidate) &&
    (!candidate.deleted || !selection?.nativeSource)
      ? candidate
      : null;
  const close = () => {
    controller.clearOpen();
    setSelection(null);
  };
  const reply = (entry: TimelineEntry) => {
    const value = current(entry);
    if (value && !value.deleted) {
      controller.clearOpen();
      onReply(value);
    }
  };
  const thread = (entry: TimelineEntry) => {
    const value = current(entry, 'thread');
    if (!value) return;
    controller.clearOpen();
    if (onThread) onThread(value);
    else
      router.push({
        pathname: '/thread/[space]/[root]',
        params: { space: spaceId, root: value.in_thread ?? value.id },
      });
  };
  const profile = async (entry: TimelineEntry) => {
    const value = current(entry);
    if (!controller.isMounted() || !value || controller.isPending() || controller.hasProfile())
      return;
    const attempt = controller.startProfile(value.id);
    const active = () =>
      controller.isMounted() && controller.isProfile(attempt) && Boolean(current(value));
    let navigated = false;
    try {
      const author = await resolveMessageAuthor(store, spaceId, attempt.messageId, active);
      if (!author) return;
      close();
      router.push({ pathname: '/person/[id]', params: { id: author } });
      navigated = true;
    } catch (reason) {
      if (active())
        void inform(
          t('chat.profileUnavailable'),
          reason instanceof Error ? reason.message : t('chat.tryAgainInAMoment'),
        );
    } finally {
      controller.finishProfile(attempt, navigated);
    }
  };
  async function react(entry: TimelineEntry, emoji: string | null) {
    const value = current(entry);
    if (!value || value.deleted || controller.isPending()) return;
    controller.clearOpen();
    controller.beginPending();
    setBusy(true);
    setFailure(null);
    try {
      await store.request({ op: 'react', space_id: spaceId, message_id: value.id, emoji });
      if (current(value)) close();
    } catch (reason) {
      if (current(value))
        setFailure({
          scope,
          id: value.id,
          text: reason instanceof Error ? reason.message : t('chat.thisCouldNotBeCompleted'),
        });
    } finally {
      controller.endPending();
      // Release only the latch, never old content after a scope change.
      if (controller.isMounted()) setBusy(false);
    }
  }
  async function recover(entry: TimelineEntry) {
    const value = current(entry);
    if (!value || !messageCanRecoverMedia(value) || controller.isPending()) return;
    controller.beginPending();
    setBusy(true);
    try {
      await store.mediaDrafts.recoverFailed(spaceId, value.in_thread, value.id, () => {
        const latest = controller.isMounted() ? current(value) : undefined;
        return latest !== undefined && messageCanRecoverMedia(latest);
      });
    } catch (reason) {
      if (controller.isMounted() && current(value))
        void inform(
          t('media.recoveryUnavailable'),
          reason instanceof Error ? reason.message : t('chat.tryAgain'),
        );
    } finally {
      controller.endPending();
      if (controller.isMounted()) setBusy(false);
    }
  }
  return {
    busy,
    selectedId: selected?.id ?? null,
    open: async (
      entry: TimelineEntry,
      actionMode: MessageActionMode,
      anchor?: MessageAnchor,
      emoji?: string,
      measureAnchor?: MeasureMessageAnchor,
      nativeSource?: MessageContextRequest,
    ) => {
      const purpose = actionMode === 'actions' ? 'delete' : 'content';
      const value = current(entry, purpose);
      if (
        !controller.isMounted() ||
        !value ||
        (value.deleted && Boolean(nativeSource)) ||
        controller.isPending() ||
        controller.hasProfile() ||
        (nativeSource && !nativeSource.current())
      )
        return;
      if (nativeSource?.kind === 'double') {
        // Native admission fences a touch; current() rechecks account, access, message and focus.
        const attempt = controller.startOpen(value.id, actionMode, emoji);
        try {
          const admitted = await nativeSource.native.consumeGesture(
            nativeSource.binding,
            nativeSource.gestureId,
          );
          const canonical = current(value, purpose);
          if (
            admitted &&
            controller.isMounted() &&
            controller.isOpen(attempt) &&
            nativeSource.current() &&
            canonical &&
            !controller.isPending()
          ) {
            // Product default only; no invented saved reaction preference.
            void react(
              canonical,
              canonical.reactions.some((reaction) => reaction.mine && reaction.emoji === '❤️')
                ? null
                : '❤️',
            );
          }
        } catch (reason) {
          if (
            controller.isMounted() &&
            controller.isOpen(attempt) &&
            nativeSource.current() &&
            current(value, purpose)
          )
            void inform(
              t('chat.thisCouldNotBeCompleted'),
              reason instanceof Error ? reason.message : t('chat.tryAgain'),
            );
        } finally {
          controller.finishOpen(attempt);
        }
        return;
      }
      if (controller.opening(value.id, actionMode, emoji)) return;
      const attempt = controller.startOpen(value.id, actionMode, emoji);
      const active = () =>
        controller.isMounted() &&
        controller.isOpen(attempt) &&
        !controller.isPending() &&
        Boolean(current(value, purpose)) &&
        (!nativeSource || nativeSource.current());
      try {
        // A native modal hides the keyboard. Finish that movement before
        // measuring the real bubble, rather than retaining its lifted position.
        await KeyboardController.dismiss();
        if (!active()) return;
        const measured = measureAnchor ? await measureAnchor() : anchor;
        if (!active() || KeyboardController.isVisible()) return;
        if (
          (measureAnchor && !measured) ||
          (measured &&
            (![measured.x, measured.y, measured.width, measured.height].every(Number.isFinite) ||
              measured.width <= 0 ||
              measured.height <= 0))
        )
          return;
        setFailure(null);
        setSelection({
          scope,
          id: value.id,
          mode: actionMode,
          anchor: measured ?? undefined,
          emoji,
          nativeSource,
        });
      } catch (reason) {
        if (active())
          void inform(
            t('chat.theMessageCouldNotBeOpened'),
            reason instanceof Error ? reason.message : t('chat.tryAgainInAMoment'),
          );
      } finally {
        controller.finishOpen(attempt);
      }
    },
    close,
    reply,
    thread,
    profile,
    react,
    panel:
      selection?.mode === 'participants' || selection?.mode === 'emoji' ? (
        <ReactionDetails
          key={`${scope}:${selection.id}`}
          entry={selected}
          mode={selection.mode}
          selectedEmoji={selection.emoji}
          canGoBack={selection.fromParticipants}
          pending={busy}
          error={failure?.scope === scope && failure.id === selected?.id ? failure.text : undefined}
          onClose={() =>
            setSelection((previous) =>
              previous?.scope === scope &&
              previous.id === selection.id &&
              (previous.mode === 'participants' || previous.mode === 'emoji')
                ? null
                : previous,
            )
          }
          onSelectEmoji={(emoji) => {
            const value = selected && current(selected);
            if (
              !value ||
              controller.isPending() ||
              !value.reactions.some((reaction) => reaction.emoji === emoji)
            )
              return;
            setSelection((previous) =>
              previous?.scope === scope && previous.id === value.id
                ? { ...previous, emoji }
                : previous,
            );
          }}
          onChooseEmoji={() => {
            const value = selected && current(selected);
            if (!value || controller.isPending()) return;
            setSelection((previous) =>
              previous?.scope === scope && previous.id === value.id
                ? { ...previous, mode: 'emoji', fromParticipants: true }
                : previous,
            );
          }}
          onBack={() => {
            const value = selected && current(selected);
            if (!value || controller.isPending()) return;
            setSelection((previous) =>
              previous?.scope === scope && previous.id === value.id
                ? { ...previous, mode: 'participants' }
                : previous,
            );
          }}
          onReact={(emoji) => {
            if (selected) void react(selected, emoji);
          }}
        />
      ) : (
        <MessageActions
          entry={selected}
          mode={selection?.mode === 'reactions' ? 'reactions' : 'actions'}
          anchor={selection?.anchor}
          nativeSource={selection?.nativeSource}
          isCurrent={() =>
            Boolean(
              selected && current(selected, selection?.mode === 'actions' ? 'delete' : 'content'),
            )
          }
          pending={busy}
          error={failure?.scope === scope && failure.id === selected?.id ? failure.text : undefined}
          onClose={close}
          onReact={(emoji) => {
            if (selected) void react(selected, emoji);
          }}
          onMore={() => {
            const value = selected && current(selected);
            if (value && !controller.isPending())
              setSelection({ scope, id: value.id, mode: 'emoji', anchor: selection?.anchor });
          }}
          onReply={() => {
            if (selected) reply(selected);
          }}
          onInfo={
            selected && !selected.deleted && mode === 'account'
              ? () => messageInfoAction(store, spaceId, selected, current, infoEpoch)()
              : undefined
          }
          onCopyLink={
            selected && !selected.deleted && 'Message' in selected.kind && mode === 'account'
              ? () => copyMessageLink(store, spaceId, selected, current, infoEpoch, inform)
              : undefined
          }
          onQuoteReview={quote.review}
          onQuote={quote.accept}
          onSelect={
            selected && messageCanSelect(selected) && onSelect
              ? () => {
                  const value = current(selected, 'delete');
                  if (value && messageCanSelect(value)) onSelect(value);
                }
              : undefined
          }
          onSelectText={
            process.env.EXPO_OS === 'ios' &&
            mode === 'account' &&
            selection?.nativeSource?.textSelection &&
            messageCanSelectText(selected)
              ? quote.open
              : undefined
          }
          onThread={() => {
            if (selected) thread(selected);
          }}
          onForward={
            selected && !selected.deleted
              ? () => {
                  if (current(selected))
                    router.push({
                      pathname: '/forward/[space]/[message]',
                      params: { space: spaceId, message: selected.id },
                    });
                }
              : undefined
          }
          onCopy={
            selected && copyableMessageText(selected)
              ? () => {
                  const value = current(selected);
                  if (value) copyMessageText(value);
                }
              : undefined
          }
          onEdit={
            selected && !messageCanRecoverMedia(selected) && messageCanEdit(selected)
              ? () => {
                  const value = current(selected);
                  if (value && messageCanEdit(value)) onEdit(value);
                }
              : undefined
          }
          onRecover={
            selected &&
            messageCanRecoverMedia(selected) &&
            mode === 'account' &&
            store.driver.media?.drafts?.recoverFailed
              ? recover
              : undefined
          }
          onDelete={
            canDeleteMessage(store, spaceId, selected)
              ? () =>
                  controller.delete(selected, choose, (entry) => current(entry, 'delete'), setBusy)
              : undefined
          }
          onShare={
            selected &&
            (selected.voice || messageCard(selected)?.kind_id === 'file') &&
            store.driver.media
              ? () => {
                  const media = store.driver.media;
                  if (media && current(selected))
                    void shareMessageMedia(selected, media, () => Boolean(current(selected))).catch(
                      (reason: unknown) => {
                        if (current(selected))
                          void inform(
                            t('media.thisFileCouldNotBeOpened'),
                            reason instanceof Error ? reason.message : t('chat.tryAgain'),
                          );
                      },
                    );
                }
              : undefined
          }
        />
      ),
  };
}
