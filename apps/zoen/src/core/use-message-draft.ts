import { t } from '../i18n/index.ts';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
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
  projectMessageDraft,
  type MessageDraftAction,
  type MessageDraftAttempt,
} from './message-draft';
import { messageCard } from './message-presentation';
import type { TimelineEntry, TimelinePage } from './types';

const subscribeEmpty = () => () => {};
const readEmpty = () => null;
const scopeChanged = () =>
  Object.assign(new Error(t('chat.editSessionChanged')), { code: 'ERR_ZOEN_DRAFT_SCOPE_CHANGED' });

/** The canonical composer owns normal text, reply and reviewed edit independently. */
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
  const mounted = useRef<typeof channel>(null);
  useLayoutEffect(() => {
    mounted.current = channel;
    return () => {
      mounted.current = null;
    };
  }, [channel]);
  const normal = useSyncExternalStore(
    channel?.controller.subscribe ?? subscribeEmpty,
    channel ? () => channel.controller.getSnapshot(channel.scope) : readEmpty,
    channel ? () => channel.controller.getSnapshot(channel.scope) : readEmpty,
  );
  const edit = useMemo(
    () => ({
      ...initialMessageDraft(),
      editing: normal?.edit
        ? {
            id: normal.edit.message_id,
            text_revision: normal.edit.source_revision,
            isCaption: normal.edit.is_caption,
          }
        : null,
      editText: normal?.edit?.text ?? '',
      editBaseText: normal?.edit?.base_text ?? '',
      contextRevision: normal?.editContextRevision ?? 0,
      editRevision: normal?.editRevision ?? 0,
    }),
    [normal],
  );
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
    if (!channel || mounted.current !== channel || !store.messageDrafts.isCurrent(channel)) return;
    switch (action.type) {
      case 'text':
        if (channel.controller.getSnapshot(channel.scope).edit)
          channel.controller.setEditText(channel.scope, action.value);
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
        }
        break;
      case 'edit': {
        const old = channel.controller.getSnapshot(channel.scope).edit;
        channel.controller.setEdit(channel.scope, {
          message_id: action.entry.id,
          source_revision: action.entry.text_revision ?? action.entry.id,
          base_text: action.text,
          text:
            old?.message_id === action.entry.id && old.text !== old.base_text
              ? old.text
              : action.text,
          is_caption: messageCard(action.entry)?.kind_id === 'file' && !action.entry.voice,
        });
        break;
      }
      case 'cancel':
        if (channel.controller.getSnapshot(channel.scope).edit)
          channel.controller.setEdit(channel.scope, null);
        else channel.controller.setReply(channel.scope, null);
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
      if (!channel || mounted.current !== channel || !store.messageDrafts.isCurrent(channel))
        return null;
      const normal = channel.controller.getSnapshot(channel.scope);
      if (!normal.loaded) return null;
      return {
        text: normal.edit ? normal.edit.text : normal.text,
        editingId: normal.edit?.message_id ?? null,
        editingRevision: normal.edit?.source_revision ?? null,
        replyId: normal.replyTo,
        contextRevision: normal.editContextRevision,
        textRevision: normal.textRevision,
        editRevision: normal.editRevision,
      };
    },
    mediaAccepted: () => {
      if (!channel || !normal || !store.messageDrafts.isCurrent(channel)) return;
      const current = channel.controller.getSnapshot(channel.scope);
      if (
        canConsumeMediaReply(
          {
            replyRevision: current.replyRevision,
            editContextRevision: current.editContextRevision,
            editing: Boolean(current.edit),
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
      if (channel && mounted.current === channel && store.messageDrafts.isCurrent(channel))
        void channel.controller.retry(channel.scope).catch(() => {});
    },
    flush: () => {
      if (channel && mounted.current === channel && store.messageDrafts.isCurrent(channel))
        void channel.controller.flush(channel.scope).catch(() => {});
    },
    send: async () => {
      if (!channel || mounted.current !== channel || !store.messageDrafts.isCurrent(channel))
        throw scopeChanged();
      const entry = await channel.controller.send(channel.scope);
      // Acceptance already consumed exactly its draft. Refresh failure must not
      // turn a committed send into another send attempt.
      if (store.messageDrafts.isCurrent(channel)) void store.refresh().catch(() => {});
      return entry;
    },
    isCurrent: (attempt?: MessageDraftAttempt) => {
      if (!channel || mounted.current !== channel || !store.messageDrafts.isCurrent(channel))
        return false;
      const current = channel.controller.getSnapshot(channel.scope);
      return (
        !attempt?.editingId ||
        (current.edit?.message_id === attempt.editingId &&
          current.editContextRevision === attempt.contextRevision)
      );
    },
    sendEdit: async (attempt: MessageDraftAttempt) => {
      if (!channel || mounted.current !== channel || !store.messageDrafts.isCurrent(channel))
        throw scopeChanged();
      const current = channel.controller.getSnapshot(channel.scope);
      if (
        !attempt.editingId ||
        !attempt.editingRevision ||
        current.edit?.message_id !== attempt.editingId ||
        current.edit.source_revision !== attempt.editingRevision ||
        current.editContextRevision !== attempt.contextRevision ||
        current.editRevision !== attempt.editRevision ||
        current.edit.text !== attempt.text
      )
        throw new Error(t('chat.editChanged'));
      const entry = await channel.controller.sendEdit(channel.scope);
      if (store.messageDrafts.isCurrent(channel)) void store.refresh().catch(() => {});
      return entry;
    },
  };
}
