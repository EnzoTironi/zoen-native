import {
  createContext,
  use,
  useCallback,
  useEffect,
  useLayoutEffect,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import { AppState } from 'react-native';
import { randomUUID } from 'expo-crypto';
import { useFocusEffect } from 'expo-router';
import { AttachmentKeyboardSession } from './attachment-keyboard-session';
import { EmojiKeyboardPanel } from './emoji-keyboard-panel';

const KeyboardPanelContext = createContext<AttachmentKeyboardSession | null>(null);
export function AttachmentKeyboardProvider({
  scope,
  enabled,
  children,
}: {
  scope: string;
  enabled: boolean;
  children: ReactNode;
}) {
  const [session] = useState(() => new AttachmentKeyboardSession(scope, enabled, randomUUID));
  useLayoutEffect(() => {
    session.configure(scope, enabled);
  }, [session, scope, enabled]);
  useFocusEffect(
    useCallback(() => {
      session.setFocused(true);
      return () => session.setFocused(false);
    }, [session]),
  );
  useEffect(() => {
    session.setForeground(
      AppState.currentState === 'active',
      AppState.currentState === 'background',
    );
    const listener = AppState.addEventListener('change', (state) =>
      session.setForeground(state === 'active', state === 'background'),
    );
    return () => {
      listener.remove();
      session.retire();
    };
  }, [session]);
  return (
    <KeyboardPanelContext value={session}>
      {children}
      <EmojiKeyboardPanel session={session} />
    </KeyboardPanelContext>
  );
}
export function useAttachmentKeyboardField(
  tag: number | null,
  context: string,
  label: string,
  placeholder = '',
) {
  const session = use(KeyboardPanelContext);
  useLayoutEffect(() => {
    if (session && tag && label) return session.bind({ tag, context, label, placeholder });
  }, [session, tag, context, label, placeholder]);
}
export function useAttachmentKeyboardSession() {
  return use(KeyboardPanelContext);
}
const empty = () => null;
const subscribeEmpty = () => () => {};
export function useAttachmentKeyboard() {
  const session = use(KeyboardPanelContext);
  const state = useSyncExternalStore(
    session?.subscribe ?? subscribeEmpty,
    session?.getSnapshot ?? empty,
    session?.getSnapshot ?? empty,
  );
  return session
    ? {
        kind: state?.presentation?.kind ?? null,
        toggleEmoji: session.toggleEmoji,
        closeKeyboardPanel: session.closeKeyboardPanel,
      }
    : null;
}
