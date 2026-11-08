import { useEffect, useRef, useState } from 'react';
import { useClient } from '../core/provider';
import type { MessagePinsSession } from './session';
import { openPinnedMessage } from './navigation';
/** One navigation per activation; a stale completion cannot dismiss or advance a new view. */
export function usePinnedNavigation(
  session: MessagePinsSession,
  generation: number,
  space: string,
  reveal?: (id: string) => Promise<void>,
) {
  const { store } = useClient();
  const flight = useRef<object | null>(null);
  const [state, setState] = useState<{
    session: MessagePinsSession;
    generation: number;
    pending: boolean;
    error: string | null;
  } | null>(null);
  useEffect(() => {
    flight.current = null;
    return () => {
      flight.current = null;
    };
  }, [session, generation]);
  const current = () => session.current(generation);
  return {
    pending: state?.session === session && state.generation === generation && state.pending,
    error: state?.session === session && state.generation === generation ? state.error : null,
    async open(message: string) {
      if (!current() || flight.current) return false;
      const attempt = {};
      flight.current = attempt;
      setState({ session, generation, pending: true, error: null });
      try {
        return await openPinnedMessage(
          store,
          space,
          message,
          () => current() && flight.current === attempt,
          reveal,
        );
      } catch {
        if (current() && flight.current === attempt)
          setState({ session, generation, pending: false, error: message });
        return false;
      } finally {
        if (flight.current === attempt) {
          flight.current = null;
          if (current())
            setState((previous) =>
              previous?.session === session && previous.generation === generation
                ? { ...previous, pending: false }
                : previous,
            );
        }
      }
    },
  };
}
