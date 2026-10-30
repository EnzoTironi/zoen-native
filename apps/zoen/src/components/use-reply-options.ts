import { useCallback, useEffect, useRef, useState } from 'react';
import { router, useFocusEffect } from 'expo-router';
import type { TimelineEntry } from '../core/types';
import type { ReplySelection } from '../core/generated/ReplySelection';
import type { NormalDraftChannel } from '../core/normal-message-draft-service';
import { captureReplyOptions } from '../core/reply-options';
import { selectedMessageQuote } from '../core/selected-message-quote';
import { useClient } from '../core/provider';
import { homeSpacesFor } from '../home/access';
import { t } from '../i18n';

type Session = {
  fence: ReturnType<typeof captureReplyOptions>;
  channel: NormalDraftChannel;
  sourceSpace: string;
  sourceId: string;
  revision: string | null;
  epoch: number;
  entry: TimelineEntry | null;
  detach: () => void;
  admitted: () => boolean;
  invalidated: () => void;
};

export function useReplyOptions(spaceId: string, threadRootId: string | null) {
  const { store } = useClient();
  const active = useRef<Session | null>(null);
  const focused = useRef(false);
  const shown = useRef(false);
  const working = useRef(false);
  const afterDismiss = useRef<(() => void) | null>(null);
  const [entry, setEntry] = useState<TimelineEntry | null>(null);
  const [selection, setSelection] = useState<ReplySelection | null>(null);
  const [selecting, setSelecting] = useState(false);
  const [canMove, setCanMove] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const retire = useCallback(() => {
    active.current?.fence.retire();
    active.current?.detach();
    active.current = null;
    afterDismiss.current = null;
    shown.current = false;
    working.current = false;
    setPending(false);
    setEntry(null);
  }, []);
  useFocusEffect(
    useCallback(() => {
      focused.current = true;
      return () => {
        focused.current = false;
        retire();
      };
    }, [retire]),
  );
  useEffect(() => () => retire(), [retire, store, spaceId, threadRootId]);
  const current = (s: Session) => active.current === s && focused.current && s.fence.current();
  const fail = (reason: unknown) =>
    setError(reason instanceof Error ? reason.message : t('replyOptions.changed'));
  async function open() {
    retire();
    const channel = store.messageDrafts.channel(spaceId, threadRootId);
    if (!focused.current || !channel || !store.driver.durableMessageDrafts) return;
    const value = channel.controller.getSnapshot(channel.scope);
    const sourceId = value.sharedReply?.message_id ?? value.replyTo;
    if (!sourceId) return;
    const sourceSpace = value.sharedReply?.space_id ?? spaceId;
    const epoch = store.sessionEpoch;
    const admitted = () => {
      const state = store.getSnapshot();
      const cached = state.timelines[sourceSpace]?.entries.find((e) => e.id === sourceId);
      return (
        focused.current &&
        store.sessionEpoch === epoch &&
        store.messageDrafts.isCurrent(channel) &&
        homeSpacesFor(state).has(sourceSpace) &&
        !cached?.deleted &&
        (!cached || !s.revision || (cached.text_revision ?? cached.id) === s.revision)
      );
    };
    const invalidated = () => {
      if (active.current === s) {
        retire();
        setError(t('replyOptions.changed'));
      }
    };
    const s: Session = {
      channel,
      sourceId,
      sourceSpace,
      epoch,
      entry: null,
      revision: value.sharedReply?.revision ?? value.replyQuote?.revision ?? null,
      detach: () => {},
      admitted,
      invalidated,
      fence: captureReplyOptions(channel, admitted, invalidated),
    };
    active.current = s;
    s.detach = store.subscribe(() => {
      if (!current(s)) {
        retire();
        setError(t('replyOptions.changed'));
      }
    });
    setError(null);
    setSelecting(false);
    setPending(true);
    working.current = true;
    try {
      const source = await store.driver.request({
        op: 'message',
        space_id: sourceSpace,
        message_id: sourceId,
      });
      if (!current(s)) return;
      if (source.deleted || (s.revision && source.text_revision !== s.revision))
        throw new Error(t('replyOptions.changed'));
      s.entry = source;
      s.revision = source.text_revision ?? source.id;
      setSelection(value.sharedReply?.selection ?? value.replyQuote);
      setCanMove(!value.sharedReply);
      setEntry(source);
      shown.current = true;
    } catch (reason) {
      if (current(s)) fail(reason);
    } finally {
      if (active.current === s) {
        working.current = false;
        setPending(false);
      }
    }
  }
  async function review(text: string, ranges?: string) {
    const s = active.current;
    if (!s?.entry || !shown.current || !current(s) || working.current) return;
    working.current = true;
    setPending(true);
    setError(null);
    try {
      const quote = selectedMessageQuote(s.entry, text, ranges);
      if (!quote) throw new Error(t('chat.quoteSelectionUnavailable'));
      const { owner } = s.channel.scope;
      const checked = s.fence.initial.sharedReply
        ? (
            await store.driver.request({
              op: 'review_shared_reply',
              expected_identity_id: owner.identityId,
              expected_device_id: owner.deviceId,
              space_id: spaceId,
              thread_root_id: null,
              source_space_id: s.sourceSpace,
              message_id: s.sourceId,
              expected_source_revision: quote.revision,
              selection: quote,
            })
          ).selection
        : await store.driver.request({
            op: 'review_reply_selection',
            expected_identity_id: owner.identityId,
            expected_device_id: owner.deviceId,
            space_id: s.sourceSpace,
            thread_root_id: s.fence.initial.sharedReply ? null : threadRootId,
            selection: quote,
          });
      if (!checked) throw new Error(t('chat.quoteSelectionUnavailable'));
      if (current(s) && shown.current) {
        setSelection(checked);
        setSelecting(false);
      }
    } catch (reason) {
      if (current(s) && shown.current) fail(reason);
    } finally {
      if (active.current === s) {
        working.current = false;
        setPending(false);
      }
    }
  }
  async function apply(quote: ReplySelection | null, move = false) {
    const s = active.current;
    if (!s?.revision || !current(s) || working.current) return;
    const sourceRevision = s.revision;
    working.current = true;
    setPending(true);
    setError(null);
    try {
      const { controller, scope } = s.channel;
      let shared = null;
      if (s.fence.initial.sharedReply) {
        shared = await store.driver.request({
          op: 'review_shared_reply',
          expected_identity_id: scope.owner.identityId,
          expected_device_id: scope.owner.deviceId,
          space_id: scope.spaceId,
          thread_root_id: null,
          source_space_id: s.sourceSpace,
          message_id: s.sourceId,
          expected_source_revision: sourceRevision,
          selection: quote ?? undefined,
        });
        if (!current(s)) return;
      }
      s.fence.retire();
      const updated = shared
        ? controller.setSharedReply(scope, shared)
        : controller.setReply(scope, s.sourceId, quote);
      // Adopt only our own mutation; later reply changes still retire this interaction.
      s.fence = captureReplyOptions(s.channel, s.admitted, s.invalidated);
      if (!updated) throw new Error(t('replyOptions.changed'));
      const replyRevision = controller.getSnapshot(scope).replyRevision;
      await controller.flush(scope);
      const value = controller.getSnapshot(scope);
      if (!current(s) || value.replyRevision !== replyRevision) return;
      setEntry(null);
      shown.current = false;
      if (move)
        afterDismiss.current = () => {
          if (
            !current(s) ||
            controller.getSnapshot(scope).persistedRevision !== value.persistedRevision
          )
            return;
          router.push({
            pathname: '/reply/[space]/[message]',
            params: {
              space: s.sourceSpace,
              message: s.sourceId,
              revision: sourceRevision,
              draftSpace: spaceId,
              draftThread: threadRootId ?? '',
              draftRevision: value.persistedRevision,
            },
          });
        };
    } catch (reason) {
      if (active.current === s && focused.current) fail(reason);
    } finally {
      if (active.current === s) {
        working.current = false;
        setPending(false);
      }
    }
  }
  function cancelReply() {
    const s = active.current;
    if (s && current(s)) {
      s.fence.retire();
      s.channel.controller.setReply(s.channel.scope, null);
    }
    retire();
  }
  return {
    supported: Boolean(store.driver.durableMessageDrafts),
    entry,
    selection,
    selecting,
    pending,
    error,
    open: () => {
      void open();
    },
    close: () => {
      shown.current = false;
      setEntry(null);
      const completed = afterDismiss.current;
      afterDismiss.current = null;
      completed?.();
      retire();
    },
    select: () => {
      const s = active.current;
      if (s && shown.current && current(s)) setSelecting(true);
    },
    review: (text: string, ranges?: string) => {
      void review(text, ranges);
    },
    apply: () => {
      void apply(selection);
    },
    remove: () => {
      void apply(null);
    },
    move: () => {
      void apply(selection, true);
    },
    cancelReply,
    canMove,
  };
}
