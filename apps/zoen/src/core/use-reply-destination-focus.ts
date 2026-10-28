import { useCallback, useRef, type RefObject } from 'react';
import { useFocusEffect } from 'expo-router';
import type { TextInputInstance } from 'react-native';

/** The destination sheet must finish handing focus back before the composer asks
 * for the keyboard. A restored draft alone never steals keyboard focus. */
export function useReplyDestinationFocus(
  token: string,
  ready: boolean,
  input: RefObject<TextInputInstance | null>,
) {
  const consumed = useRef<string | null>(null);
  useFocusEffect(
    useCallback(() => {
      if (!ready || consumed.current === token) return;
      const frame = requestAnimationFrame(() => {
        if (!input.current) return;
        consumed.current = token;
        input.current.focus();
      });
      return () => cancelAnimationFrame(frame);
    }, [token, ready, input]),
  );
}
