import { useLayoutEffect, useMemo, useRef } from 'react';
import * as Clipboard from 'expo-clipboard';
import { t } from '../i18n/index.ts';
import { useDialogs } from './dialogs';

/** Keep asynchronous clipboard feedback attached to the rendered message lifetime. */
export function useMessageCodeCopy(source: string, identity?: string) {
  const dialogs = useDialogs();
  const lifetime = useRef<{ source: string; identity?: string } | null>(null);
  const owner = useMemo(() => ({ source, identity }), [source, identity]);
  useLayoutEffect(() => {
    lifetime.current = owner;
    return () => {
      if (lifetime.current === owner) lifetime.current = null;
    };
  }, [owner]);

  return async (code: string) => {
    if (lifetime.current !== owner) return;
    let copied = false;
    try {
      copied = (await Clipboard.setStringAsync(code)) === true;
    } catch {
      // A rejected or refused write gets the same feedback.
    }
    if (lifetime.current === owner)
      await dialogs.inform(t(copied ? 'chat.codeCopied' : 'chat.copyCodeFailed'));
  };
}
