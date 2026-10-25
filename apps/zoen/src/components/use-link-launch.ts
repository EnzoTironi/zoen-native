import { useLayoutEffect, useRef } from 'react';
import { Linking } from 'react-native';
import * as Clipboard from 'expo-clipboard';
import { t } from '../i18n/index.ts';
import { useDialogs } from './dialogs';

/** Call only after the caller's navigation boundary has validated the target. */
export function useLinkLaunch(source: string) {
  const dialogs = useDialogs();
  const lifetime = useRef<{ source: string; live: boolean; prompting: boolean } | null>(null);
  useLayoutEffect(() => {
    const owner = { source, live: true, prompting: false };
    lifetime.current = owner;
    return () => {
      owner.live = false;
      if (lifetime.current === owner) lifetime.current = null;
    };
  }, [source]);

  return async (target: string): Promise<void> => {
    const owner = lifetime.current;
    if (!owner?.live || owner.source !== source) return;
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
