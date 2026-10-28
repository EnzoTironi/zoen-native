import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useFocusEffect } from 'expo-router';
import { markdownToPlainText } from 'react-native-enriched-markdown';
import { conversationScope } from '../core/conversation-scope';
import { accountScope, accountScopeKey } from '../core/account-scope';
import { useClient } from '../core/provider';
import type { ClientState, ClientStore } from '../core/store';
import { t } from '../i18n/index.ts';

/** Presentation input only. The driver owns message access and the native AST owns syntax. */
type Source = {
  owner: string;
  text: string;
  literal: boolean;
  enabled?: boolean;
  target?: { spaceId: string; messageId?: string };
};
function session(store: ClientStore, state: ClientState, space?: string): string {
  if (store.isSessionRetiring) return '';
  if (space) return conversationScope(state, store.sessionEpoch, space) ?? '';
  const account = accountScopeKey(accountScope(store, state));
  if (account) return account;
  return state.phase === 'ready' &&
    state.mode === 'demo' &&
    !state.erasePending &&
    !store.isSessionRetiring
    ? JSON.stringify(['demo', store.sessionEpoch])
    : '';
}

/** One transient receipt for the visible preview; no message/draft or global cache is changed. */
export function useSemanticMessagePreview({
  owner: key,
  text,
  literal,
  target,
  enabled = true,
}: Source) {
  const client = useClient();
  const { store } = client;
  const scope = session(store, client, target?.spaceId);
  const page = target ? client.timelines[target.spaceId] : undefined;
  const owner = useMemo(
    () => ({
      key,
      text,
      literal,
      space: target?.spaceId,
      id: target?.messageId,
      page,
      snapshot: client.snapshot,
      scope,
      enabled,
    }),
    [key, text, literal, target?.spaceId, target?.messageId, page, client.snapshot, scope, enabled],
  );
  const live = useRef<typeof owner | null>(null);
  const [receipt, setReceipt] = useState<{ owner: typeof owner; text: string } | null>(null);
  useLayoutEffect(() => {
    live.current = owner;
    return () => {
      if (live.current === owner) live.current = null;
    };
  }, [owner]);
  useFocusEffect(
    useCallback(() => {
      let active = true;
      const current = () =>
        active &&
        live.current === owner &&
        owner.scope !== '' &&
        session(store, store.getSnapshot(), owner.space) === owner.scope &&
        store.getSnapshot().snapshot === owner.snapshot &&
        (!owner.space || store.getSnapshot().timelines[owner.space] === owner.page);
      if (!enabled || literal || process.env.EXPO_OS !== 'ios' || !current()) return;
      // Observe transient account/source replacement even if it returns before React renders.
      const detach = store.subscribe(() => {
        if (!current()) active = false;
      });
      const detachCanonical = store.subscribeCanonicalChanges((spaces) => {
        if (owner.space && (!spaces || spaces.includes(owner.space))) {
          active = false;
          setReceipt({ owner, text: t('chat.messageUnavailable') });
        }
      });
      void (async () => {
        let source = owner.text;
        let plain = false;
        if (owner.space && owner.id) {
          // Read through Core even if cached: a snapshot can refresh before its timeline.
          const entry = await store.driver.request({
            op: 'message',
            space_id: owner.space,
            message_id: owner.id,
          });
          if (!current()) return;
          if (entry.id !== owner.id) throw new Error('Quoted message changed');
          if (entry.deleted) {
            source = t('presentation.deletedMessage');
            plain = true;
          } else if ('Message' in entry.kind && entry.kind.Message.text.length > 0) {
            // reply_to.text is an excerpt. Only the whole canonical message can be parsed.
            source = entry.kind.Message.text;
          } else {
            // Generated attachment/voice labels are already literal presentation text.
            plain = true;
          }
        }
        if (!current()) return;
        const projected = plain ? source : await markdownToPlainText(source);
        if (typeof projected !== 'string') throw new Error('Invalid native text projection');
        if (current()) setReceipt({ owner, text: projected });
      })().catch(() => {
        if (current()) setReceipt({ owner, text: t('chat.messageUnavailable') });
      });
      return () => {
        active = false;
        detach();
        detachCanonical();
      };
    }, [owner, store, enabled, literal]),
  );
  return {
    text:
      !enabled || literal || process.env.EXPO_OS !== 'ios'
        ? text
        : !scope
          ? t('chat.messageUnavailable')
          : receipt?.owner === owner
            ? receipt.text
            : undefined,
  };
}
