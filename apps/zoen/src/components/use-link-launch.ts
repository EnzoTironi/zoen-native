import { use, useLayoutEffect, useRef } from 'react';
import { ConversationHashtagSearch } from './conversation-hashtag-search';
import { messageHashtagLink } from '../core/message-hashtag';
import { Linking } from 'react-native';
import * as Clipboard from 'expo-clipboard';
import { t } from '../i18n/index.ts';
import { useDialogs } from './dialogs';

/** Call only after the caller's navigation boundary has validated the target. */
export function useLinkLaunch(source: string) {
  const dialogs = useDialogs();
  const hashtagSearch = use(ConversationHashtagSearch);
  const lifetime = useRef<{
    source: string;
    live: boolean;
    prompting: boolean;
    hashtagSearch: typeof hashtagSearch;
  } | null>(null);
  useLayoutEffect(() => {
    const owner = { source, live: true, prompting: false, hashtagSearch };
    lifetime.current = owner;
    return () => {
      owner.live = false;
      if (lifetime.current === owner) lifetime.current = null;
    };
  }, [source, hashtagSearch]);

  return async (target: string): Promise<void> => {
    const owner = lifetime.current;
    if (!owner?.live || owner.source !== source || owner.hashtagSearch !== hashtagSearch) return;
    const tag = messageHashtagLink(target);
    if (tag) {
      hashtagSearch?.(tag);
      return;
    }
    try {
      await Linking.openURL(target);
      return;
    } catch {
      if (!owner.live || owner.prompting) return;
    }
    owner.prompting = true;
    try {
      const copy = await dialogs.confirm({
        title: t('chat.linkUnavailable'),
        message: target,
        confirmLabel: t('chat.copyLink'),
      });
      if (!copy || !owner.live) return;
      let copied = false;
      try {
        copied = (await Clipboard.setStringAsync(target)) === true;
      } catch {
        // Rejected and refused clipboard writes share the same visible failure.
      }
      if (!copied && owner.live) await dialogs.inform(t('chat.copyLinkFailed'), target);
    } finally {
      owner.prompting = false;
    }
  };
}
