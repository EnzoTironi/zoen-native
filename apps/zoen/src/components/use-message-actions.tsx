import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { router, useFocusEffect } from 'expo-router';
import * as Clipboard from 'expo-clipboard';
import { KeyboardController } from 'react-native-keyboard-controller';
import type { TimelineEntry } from '../core/types';
import { useClient } from '../core/provider';
import { messageCanEdit, messageCanInteract, messageCard } from '../core/message-presentation';
import { messageText } from '../core/presentation';
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
}: {
  spaceId: string;
  entries: readonly TimelineEntry[];
  disabled?: boolean;
  onReply: (entry: TimelineEntry) => void;
  onEdit: (entry: TimelineEntry) => void;
  onThread?: (entry: TimelineEntry) => void;
}) {
  const client = useClient();
  const { store, snapshot, mode } = client;
  const { confirm, inform } = useDialogs();
  const scope = `${mode}:${snapshot?.account?.identity_id ?? snapshot?.me?.id}:${snapshot?.account?.device_id}:${spaceId}`;
  const latest = useRef({ entries, disabled, scope });
  const focused = useRef(true);
  const mounted = useRef(false);
  const pending = useRef(false);
  const profileEpoch = useRef(0);
  const profileAttempt = useRef<{ epoch: number; scope: string; messageId: string } | null>(null);
  const openAttempt = useRef<{
    epoch: number;
    scope: string;
    messageId: string;
    mode: MessageActionMode;
    emoji?: string;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<{ scope: string; id: string; text: string } | null>(null);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      profileAttempt.current = null;
      openAttempt.current = null;
    };
  }, []);
  const [selection, setSelection] = useState<{
    scope: string;
    id: string;
    mode: MessageActionMode;
    anchor?: MessageAnchor;
    emoji?: string;
    fromParticipants?: boolean;
    nativeSource?: MessageContextRequest;
  } | null>(null);
  useLayoutEffect(() => {
    if (latest.current.scope !== scope || (disabled && !latest.current.disabled)) {
      ++profileEpoch.current;
      profileAttempt.current = null;
      openAttempt.current = null;
    }
    latest.current = { entries, disabled, scope };
  }, [entries, disabled, scope]);
  useFocusEffect(
    useCallback(() => {
      focused.current = true;
      ++profileEpoch.current;
      profileAttempt.current = null;
      openAttempt.current = null;
      setBusy(pending.current);
      return () => {
        focused.current = false;
        ++profileEpoch.current;
        profileAttempt.current = null;
        openAttempt.current = null;
        setSelection(null);
      };
    }, []),
  );
  const current = useCallback(
    (entry: TimelineEntry) => {
      const state = store.getSnapshot();
      if (
        !focused.current ||
        latest.current.disabled ||
        latest.current.scope !== scope ||
        state.phase !== 'ready' ||
        !homeSpacesFor(state).has(spaceId) ||
        `${state.mode}:${state.snapshot?.account?.identity_id ?? state.snapshot?.me?.id}:${state.snapshot?.account?.device_id}:${spaceId}` !==
          scope
      )
        return undefined;
      const value = latest.current.entries.find((value) => value.id === entry.id);
      return value && messageCanInteract(value) ? value : undefined;
    },
    [scope, spaceId, store],
  );
  useEffect(
    () =>
      store.subscribe(() => {
        for (const attempt of [profileAttempt.current, openAttempt.current]) {
          if (!attempt || attempt.scope !== scope) continue;
          const value = latest.current.entries.find((entry) => entry.id === attempt.messageId);
          if (!value || !current(value)) {
            ++profileEpoch.current;
            profileAttempt.current = null;
            openAttempt.current = null;
            return;
          }
        }
      }),
    [current, scope, store],
  );
  const candidate =
    !disabled && selection?.scope === scope
      ? (entries.find((entry) => entry.id === selection.id) ?? null)
      : null;
  const selected =
    homeSpacesFor(client).has(spaceId) && candidate && messageCanInteract(candidate)
      ? candidate
      : null;
  const close = () => {
    openAttempt.current = null;
    setSelection(null);
  };
  const reply = (entry: TimelineEntry) => {
    const value = current(entry);
    if (value && !value.deleted) {
      openAttempt.current = null;
      onReply(value);
    }
  };
  const thread = (entry: TimelineEntry) => {
    const value = current(entry);
    if (value && !value.deleted) openAttempt.current = null;
    if (value && !value.deleted && onThread) onThread(value);
    else if (value && !value.deleted)
      router.push({
        pathname: '/thread/[space]/[root]',
        params: { space: spaceId, root: value.in_thread ?? value.id },
      });
  };
  const profile = async (entry: TimelineEntry) => {
    const value = current(entry);
    if (!mounted.current || !value || pending.current || profileAttempt.current) return;
    openAttempt.current = null;
    const attempt = { epoch: profileEpoch.current, scope, messageId: value.id };
    profileAttempt.current = attempt;
    const active = () =>
      mounted.current &&
      profileAttempt.current === attempt &&
      profileEpoch.current === attempt.epoch &&
      Boolean(current(value));
    let navigated = false;
    try {
      // A recycled row supplies only message identity. Resolve the signed current
      // author again, and keep late reads from opening a different account's route.
      const canonical = await store.driver.request({
        op: 'message',
        space_id: spaceId,
        message_id: attempt.messageId,
      });
      if (!active()) return;
      if (
        canonical.id !== attempt.messageId ||
        !messageCanInteract(canonical) ||
        !canonical.author.id.trim()
      )
        throw new Error(t('chat.thisMessageIsNoLongerInThisDevice'));
      close();
      router.push({ pathname: '/person/[id]', params: { id: canonical.author.id } });
      navigated = true;
    } catch (reason) {
      if (active())
        void inform(
          t('chat.profileUnavailable'),
          reason instanceof Error ? reason.message : t('chat.tryAgainInAMoment'),
        );
    } finally {
      if (profileAttempt.current === attempt && !navigated) profileAttempt.current = null;
    }
  };
  async function react(entry: TimelineEntry, emoji: string | null) {
    const value = current(entry);
    if (!value || value.deleted || pending.current) return;
    openAttempt.current = null;
    pending.current = true;
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
      pending.current = false;
      // This releases an interaction latch, never old account content. A scope
      // change must not leave the newly focused conversation permanently busy.
      if (mounted.current) setBusy(false);
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
      const value = current(entry);
      if (
        !mounted.current ||
        !value ||
        pending.current ||
        profileAttempt.current ||
        (nativeSource && !nativeSource.current())
      )
        return;
      if (nativeSource?.kind === 'double') {
        // Exact native admission fences a held/replayed event after a NEW touch.
        // The native view has no mutation authority; current() still checks the
        // account, access, signed message and focus again after this bridge await.
        const attempt = {
          epoch: profileEpoch.current,
          scope,
          messageId: value.id,
          mode: actionMode,
          emoji,
        };
        openAttempt.current = attempt;
        try {
          const admitted = await nativeSource.native.consumeGesture(
            nativeSource.binding,
            nativeSource.gestureId,
          );
          const canonical = current(value);
          if (
            admitted &&
            mounted.current &&
            openAttempt.current === attempt &&
            profileEpoch.current === attempt.epoch &&
            nativeSource.current() &&
            canonical &&
            !pending.current
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
            mounted.current &&
            openAttempt.current === attempt &&
            nativeSource.current() &&
            current(value)
          )
            void inform(
              t('chat.thisCouldNotBeCompleted'),
              reason instanceof Error ? reason.message : t('chat.tryAgain'),
            );
        } finally {
          if (openAttempt.current === attempt) openAttempt.current = null;
        }
        return;
      }
      if (
        openAttempt.current?.messageId === value.id &&
        openAttempt.current.mode === actionMode &&
        openAttempt.current.emoji === emoji
      )
        return;
      const attempt = {
        epoch: profileEpoch.current,
        scope,
        messageId: value.id,
        mode: actionMode,
        emoji,
      };
      openAttempt.current = attempt;
      const active = () =>
        mounted.current &&
        openAttempt.current === attempt &&
        profileEpoch.current === attempt.epoch &&
        !pending.current &&
        Boolean(current(value)) &&
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
        if (openAttempt.current === attempt) openAttempt.current = null;
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
              pending.current ||
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
            if (!value || pending.current) return;
            setSelection((previous) =>
              previous?.scope === scope && previous.id === value.id
                ? { ...previous, mode: 'emoji', fromParticipants: true }
                : previous,
            );
          }}
          onBack={() => {
            const value = selected && current(selected);
            if (!value || pending.current) return;
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
          isCurrent={() => Boolean(selected && current(selected))}
          pending={busy}
          error={failure?.scope === scope && failure.id === selected?.id ? failure.text : undefined}
          onClose={close}
          onReact={(emoji) => {
            if (selected) void react(selected, emoji);
          }}
          onMore={() => {
            const value = selected && current(selected);
            if (value && !pending.current)
              setSelection({ scope, id: value.id, mode: 'emoji', anchor: selection?.anchor });
          }}
          onReply={() => {
            if (selected) reply(selected);
          }}
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
            selected &&
            !selected.voice &&
            (!messageCard(selected) || messageCard(selected)?.kind_id === 'file') &&
            messageText(selected).trim()
              ? () => {
                  if (current(selected))
                    void Clipboard.setStringAsync(messageText(selected)).catch(() => {});
                }
              : undefined
          }
          onEdit={
            selected && messageCanEdit(selected)
              ? () => {
                  const value = current(selected);
                  if (value && messageCanEdit(value)) onEdit(value);
                }
              : undefined
          }
          onDelete={
            selected?.author.is_me && !selected.deleted
              ? () => {
                  const entry = selected;
                  if (!current(entry)) return;
                  void confirm({
                    title: t('chat.deleteMessage'),
                    message: t('chat.theMessageWillBeRemovedForEveryoneIn'),
                    confirmLabel: t('chat.delete'),
                    destructive: true,
                  }).then((accepted) => {
                    const value = current(entry);
                    if (accepted && value?.author.is_me && !value.deleted)
                      void store
                        .request({ op: 'delete_message', space_id: spaceId, message_id: value.id })
                        .catch(() => {});
                  });
                }
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
