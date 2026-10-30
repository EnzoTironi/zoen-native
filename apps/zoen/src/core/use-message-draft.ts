import { t } from '../i18n/index.ts';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { AppState } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { useClient } from './provider';
import {
  canConsumeMediaReply,
  initialMessageDraft,
  messageDraftReducer,
  projectMessageDraft,
  type MessageDraftAction,
  type MessageDraftAttempt,
} from './message-draft';
import type { TimelineEntry, TimelinePage } from './types';

const empty = initialMessageDraft();
const subscribeEmpty = () => () => {};
const readEmpty = () => null;
const scopeChanged = () =>
  Object.assign(new Error(t('chat.editSessionChanged')), { code: 'ERR_ZOEN_DRAFT_SCOPE_CHANGED' });

/** Normal text is durable; edit/search/caption/recording state remains independent. */
export function useMessageDraft(spaceId: string, threadRootId: string | null = null) {
  const client = useClient();
  const { store, snapshot, timelines } = client;
  // The service survives account changes; observe the client before selecting its channel.
  const channel = useMemo(
    () =>
      client.phase === 'ready' && client.snapshot
        ? client.store.messageDrafts.channel(spaceId, threadRootId)
        : null,
    [client, spaceId, threadRootId],
  );
  const normal = useSyncExternalStore(
    channel?.controller.subscribe ?? subscribeEmpty,
    channel ? () => channel.controller.getSnapshot(channel.scope) : readEmpty,
    channel ? () => channel.controller.getSnapshot(channel.scope) : readEmpty,
  );
  const [transient, dispatch] = useReducer(
    (
      state: { channel: typeof channel; value: typeof empty },
      action: { channel: typeof channel; action: MessageDraftAction },
    ) => ({
      channel: action.channel,
      value: messageDraftReducer(
        state.channel === action.channel ? state.value : initialMessageDraft(),
        action.action,
      ),
    }),
    { channel, value: initialMessageDraft() },
  );
  const edit = transient.channel === channel ? transient.value : empty;
  const transientNow = useRef({ channel, value: edit });
  useLayoutEffect(() => {
    const current = { channel, value: edit };
    transientNow.current = current;
    return () => {
      if (transientNow.current === current) transientNow.current = { channel: null, value: empty };
    };
  }, [channel, edit]);
  const [quote, setQuote] = useState<{
    channel: typeof channel;
    id: string;
    revision: number;
    page: TimelinePage | undefined;
    entry: TimelineEntry | null;
  } | null>(null);
  const replyTo = normal?.replyTo ?? null;
  const replyRevision = normal?.replyRevision ?? 0;
  const page = timelines[spaceId];
  const cachedReply = replyTo ? page?.entries.find((entry) => entry.id === replyTo) : undefined;
  const errorCause = normal?.error?.cause;
  const rejected =
    typeof errorCause === 'object' &&
    errorCause !== null &&
    'code' in errorCause &&
    errorCause.code === 'ERR_ZOEN_DRAFT_REJECTED';
  const selectedQuote =
    quote?.channel === channel && quote.id === replyTo && quote.revision === replyRevision
      ? quote
      : null;
  const preview =
    selectedQuote && selectedQuote.page === page
      ? selectedQuote.entry
      : (cachedReply ?? selectedQuote?.entry ?? null);
  // A cached tombstone takes effect in the same render, even if a canonical
  // read is still pending. The durable quote target remains independently set.
  const reply = cachedReply?.deleted || preview?.deleted ? null : preview;
  const messageDraft = useMemo(
    () =>
      projectMessageDraft(edit, {
        text: normal?.text ?? '',
        textRevision: normal?.textRevision ?? 0,
        reply,
      }),
    [edit, normal?.text, normal?.textRevision, reply],
  );

  useEffect(() => {
    if (channel) void channel.controller.activate(channel.scope).catch(() => {});
  }, [channel]);
  useFocusEffect(
    useCallback(
      () => () => {
        if (channel) void channel.controller.flush(channel.scope).catch(() => {});
      },
      [channel],
    ),
  );
  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => {
      if (state !== 'active' && channel && store.messageDrafts.isCurrent(channel))
        void channel.controller.flush(channel.scope).catch(() => {});
    });
    return () => subscription.remove();
  }, [channel, store]);
  useEffect(() => {
    if (!channel || !replyTo || !store.messageDrafts.isCurrent(channel)) return;
    let active = true;
    const current = () => {
      if (!active || !store.messageDrafts.isCurrent(channel)) return false;
      const state = store.getSnapshot();
      const draft = channel.controller.getSnapshot(channel.scope);
      return (
        state.snapshot === snapshot &&
        state.timelines[spaceId] === page &&
        draft.replyTo === replyTo &&
        draft.replyRevision === replyRevision
      );
    };
    // Capture transient owner/access/source loss even if it changes back before
    // React renders again; an old read must not revive through that round trip.
    const detach = store.subscribe(() => {
      if (!current()) active = false;
    });
    const publish = (entry: TimelineEntry | null) => {
      if (current()) setQuote({ channel, id: replyTo, revision: replyRevision, page, entry });
    };
    void store.driver
      .request({ op: 'message', space_id: spaceId, message_id: replyTo })
      .then((entry) => publish(entry.deleted ? null : entry))
      .catch(() => publish(null));
    return () => {
      active = false;
      detach();
    };
  }, [channel, replyTo, replyRevision, snapshot, page, spaceId, store]);

  function updateDraft(action: MessageDraftAction) {
    if (!channel || !store.messageDrafts.isCurrent(channel)) return;
    function changeTransient(action: MessageDraftAction) {
      const previous = transientNow.current;
      transientNow.current = {
        channel,
        value: messageDraftReducer(
          previous.channel === channel ? previous.value : initialMessageDraft(),
          action,
        ),
      };
      dispatch({ channel, action });
    }
    switch (action.type) {
      case 'text':
        if (edit.editing) changeTransient(action);
        else channel.controller.setText(channel.scope, action.value);
        break;
      case 'reply':
        if (channel.controller.setReply(channel.scope, action.entry.id, action.selection ?? null)) {
          const current = store.getSnapshot();
          setQuote({
            channel,
            id: action.entry.id,
            revision: channel.controller.getSnapshot(channel.scope).replyRevision,
            page: current.timelines[spaceId],
            entry: action.entry.deleted ? null : action.entry,
          });
          changeTransient({ type: 'cancel' });
        }
        break;
      case 'edit':
        changeTransient(action);
        break;
      case 'cancel':
        if (edit.editing) changeTransient(action);
        else channel.controller.setReply(channel.scope, null);
        break;
      case 'media-accepted':
        // Media completion uses the independently captured fences below.
        break;
      case 'accepted':
        if (action.attempt.editingId) changeTransient(action);
        break;
    }
  }

  return {
    messageDraft,
    updateDraft,
    replyTo,
    replyQuote: normal?.replyQuote ?? null,
    sharedReply: normal?.sharedReply ?? null,
    capture: (): MessageDraftAttempt | null => {
      if (!channel || !store.messageDrafts.isCurrent(channel)) return null;
      const transient = transientNow.current;
      if (transient.channel !== channel) return null;
      const normal = channel.controller.getSnapshot(channel.scope);
      if (!normal.loaded) return null;
      return {
        text: transient.value.editing ? transient.value.editText : normal.text,
        editingId: transient.value.editing?.id ?? null,
        editingRevision:
          transient.value.editing?.text_revision ?? transient.value.editing?.id ?? null,
        replyId: normal.replyTo,
        contextRevision: transient.value.contextRevision,
        textRevision: normal.textRevision,
        editRevision: transient.value.editRevision,
      };
    },
    mediaAccepted: () => {
      if (!channel || !normal || !store.messageDrafts.isCurrent(channel)) return;
      const current = channel.controller.getSnapshot(channel.scope);
      const transient = transientNow.current;
      if (
        transient.channel === channel &&
        canConsumeMediaReply(
          {
            replyRevision: current.replyRevision,
            editContextRevision: transient.value.contextRevision,
            editing: Boolean(transient.value.editing),
          },
          {
            replyRevision: normal.replyRevision,
            editContextRevision: edit.contextRevision,
          },
        )
      )
        channel.controller.setReply(channel.scope, null);
    },
    ready: Boolean(normal?.loaded && channel && store.messageDrafts.isCurrent(channel)),
    loading: !normal || normal.loading || !normal.loaded,
    error: normal?.error ?? null,
    rejected,
    retry: () => {
      if (channel && store.messageDrafts.isCurrent(channel))
        void channel.controller.retry(channel.scope).catch(() => {});
    },
    flush: () => {
      if (channel && store.messageDrafts.isCurrent(channel))
        void channel.controller.flush(channel.scope).catch(() => {});
    },
    send: async () => {
      if (!channel || !store.messageDrafts.isCurrent(channel)) throw scopeChanged();
      const entry = await channel.controller.send(channel.scope);
      // Acceptance already consumed exactly its draft. Refresh failure must not
      // turn a committed send into another send attempt.
      if (store.messageDrafts.isCurrent(channel)) void store.refresh().catch(() => {});
      return entry;
    },
    isCurrent: (attempt?: MessageDraftAttempt) => {
      const current = transientNow.current;
      return Boolean(
        channel &&
        current.channel === channel &&
        store.messageDrafts.isCurrent(channel) &&
        (!attempt?.editingId ||
          (current.value.editing?.id === attempt.editingId &&
            current.value.contextRevision === attempt.contextRevision)),
      );
    },
    sendEdit: async (attempt: MessageDraftAttempt) => {
      if (!channel || !store.messageDrafts.isCurrent(channel)) throw scopeChanged();
      const current = transientNow.current;
      if (
        current.channel !== channel ||
        !attempt.editingId ||
        !attempt.editingRevision ||
        current.value.editing?.id !== attempt.editingId ||
        current.value.contextRevision !== attempt.contextRevision ||
        current.value.editRevision !== attempt.editRevision
      )
        throw new Error(t('chat.editChanged'));
      await store.request({
        op: 'edit_message',
        expected_identity_id: channel.scope.owner.identityId,
        expected_device_id: channel.scope.owner.deviceId,
        space_id: spaceId,
        message_id: attempt.editingId,
        expected_revision: attempt.editingRevision,
        text: attempt.text,
      });
    },
  };
}
