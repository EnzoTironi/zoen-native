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
} from './message-draft';
import type { TimelineEntry } from './types';

const empty = initialMessageDraft();
const subscribeEmpty = () => () => {};
const readEmpty = () => null;

/** Normal text is durable; edit/search/caption/recording state remains independent. */
export function useMessageDraft(spaceId: string, threadRootId: string | null = null) {
  const { store } = useClient();
  const channel = store.messageDrafts.channel(spaceId, threadRootId);
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
    transientNow.current = { channel, value: edit };
  }, [channel, edit]);
  const [quote, setQuote] = useState<{
    channel: typeof channel;
    id: string;
    entry: TimelineEntry;
  } | null>(null);
  const replyTo = normal?.replyTo ?? null;
  const errorCause = normal?.error?.cause;
  const rejected =
    typeof errorCause === 'object' &&
    errorCause !== null &&
    'code' in errorCause &&
    errorCause.code === 'ERR_ZOEN_DRAFT_REJECTED';
  const reply = quote?.channel === channel && quote.id === replyTo ? quote.entry : null;
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
    void store.driver
      .request({ op: 'message', space_id: spaceId, message_id: replyTo })
      .then((entry) => {
        if (active && store.messageDrafts.isCurrent(channel))
          setQuote({ channel, id: replyTo, entry });
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [channel, replyTo, spaceId, store]);

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
        if (channel.controller.setReply(channel.scope, action.entry.id)) {
          setQuote({ channel, id: action.entry.id, entry: action.entry });
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
      if (!channel || !store.messageDrafts.isCurrent(channel)) throw new Error('A sessão mudou.');
      const entry = await channel.controller.send(channel.scope);
      // Acceptance already consumed exactly its draft. Refresh failure must not
      // turn a committed send into another send attempt.
      if (store.messageDrafts.isCurrent(channel)) void store.refresh().catch(() => {});
      return entry;
    },
  };
}
