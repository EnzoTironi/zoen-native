import { useCallback, useLayoutEffect, useRef, useState } from 'react';
import { router, useFocusEffect } from 'expo-router';
import * as Clipboard from 'expo-clipboard';
import { Keyboard } from 'react-native';
import { conversationScope } from '../core/conversation-scope';
import {
  MAX_SELECTED_MESSAGES,
  selectedMessages,
  selectedMessageText,
  toggleMessageSelection,
  type MessageSelection,
} from '../core/message-selection';
import { useClient } from '../core/provider';
import type { TimelineEntry } from '../core/types';
import { confirmMessageDeletion, messageDeletionTargets } from '../core/message-deletion';
import { useDialogs } from './dialogs';
import { chooseMessageDeletion } from './delete-message-action';

export function useMessageSelection(
  spaceId: string,
  entries: readonly TimelineEntry[],
  disabled: boolean,
  threadRootId: string | null = null,
) {
  const client = useClient();
  const { store } = client;
  const { choose } = useDialogs();
  const confirmation = useRef<AbortController | null>(null);
  const deleting = useRef(false);
  const [pending, setPending] = useState(false);
  const scope = conversationScope(client, store.sessionEpoch, spaceId);
  const binding = scope && JSON.stringify([scope, threadRootId]);
  const [selection, setSelection] = useState<MessageSelection>(null);
  const focused = useRef(false);
  const latest = useRef({ scope, binding, entries, disabled, selection });
  useLayoutEffect(() => {
    latest.current = { scope, binding, entries, disabled, selection };
  }, [scope, binding, entries, disabled, selection]);
  useFocusEffect(
    useCallback(() => {
      focused.current = true;
      return () => {
        focused.current = false;
        confirmation.current?.abort();
        setSelection(null);
      };
    }, []),
  );
  const messages = disabled ? [] : selectedMessages(selection, binding, entries);
  const active = messages.length > 0;
  useLayoutEffect(() => {
    // oxlint-disable-next-line react/set-state-in-effect -- Reconcile selection with externally owned canonical timeline and access changes.
    setSelection((value) => {
      const ids = disabled
        ? []
        : selectedMessages(value, binding, entries).map((entry) => entry.id);
      return !ids.length
        ? null
        : value?.ids.length === ids.length
          ? value
          : { scope: binding!, ids };
    });
  }, [binding, entries, disabled]);
  const current = () =>
    focused.current &&
    !latest.current.disabled &&
    scope !== null &&
    latest.current.scope === scope &&
    latest.current.binding === binding &&
    !store.isSessionRetiring &&
    conversationScope(store.getSnapshot(), store.sessionEpoch, spaceId) === scope;
  const toggle = (entry: TimelineEntry) => {
    if (!current()) return;
    Keyboard.dismiss();
    store.setTyping(spaceId, false);
    setSelection((value) =>
      toggleMessageSelection(value, binding, latest.current.entries, entry.id),
    );
  };
  const resolve = () =>
    current() && latest.current.selection === selection
      ? selectedMessages(selection, binding, latest.current.entries)
      : [];
  const text = selectedMessageText(messages);
  const account = client.snapshot?.account;
  const canWrite = () =>
    store
      .getSnapshot()
      .snapshot?.conversations.some((chat) => chat.id === spaceId && chat.can_write) === true;
  return {
    active,
    ids: new Set(messages.map((entry) => entry.id)),
    count: messages.length,
    limitReached: messages.length >= MAX_SELECTED_MESSAGES,
    canCopy: Boolean(text),
    canForward: client.mode === 'account' && active,
    canDelete:
      client.mode === 'account' &&
      Boolean(messageDeletionTargets(messages, account?.identity_id, true)),
    pending,
    toggle,
    cancel: () => setSelection(null),
    copy: () => {
      const value = selectedMessageText(resolve());
      if (value) void Clipboard.setStringAsync(value).catch(() => {});
    },
    forward: () => {
      const values = resolve();
      if (client.mode !== 'account' || !values.length) return;
      router.push({
        pathname: '/forward-selection/[space]',
        params: {
          space: spaceId,
          messages: JSON.stringify(values.map((entry) => entry.id)),
          selectionScope: scope!,
        },
      });
    },
    remove: () => {
      if (deleting.current || !account || client.mode !== 'account') return;
      deleting.current = true;
      setPending(true);
      const controller = new AbortController();
      confirmation.current = controller;
      void confirmMessageDeletion({
        identity: account.identity_id,
        device: account.device_id,
        space: spaceId,
        resolve,
        canDeleteEveryone: canWrite,
        signal: controller.signal,
        subscribe: (listener) => store.subscribe(listener),
        confirm: (count, everyone, signal) =>
          chooseMessageDeletion(choose, count, everyone, signal),
        send: (value) => store.request({ op: 'delete_messages', ...value }),
      })
        .then((accepted) => {
          if (accepted) setSelection((value) => (value === selection ? null : value));
        })
        .catch(() => {
          // The canonical store surfaces command failures and retains selection for retry.
        })
        .finally(() => {
          if (confirmation.current === controller) confirmation.current = null;
          deleting.current = false;
          setPending(false);
        });
    },
  };
}
