import { useCallback, useState } from 'react';
import { useFocusEffect } from 'expo-router';
import { useClient } from './provider';
import type { Command, CommandResult } from './types';

/** Focus-scoped projections; old routes and accounts cannot publish late results. */
export function useCoreQuery<C extends Command>(command: C | null) {
  const { store, snapshot, mode } = useClient();
  const encoded = JSON.stringify(command);
  const scope = `${mode}:${snapshot?.account?.identity_id ?? ''}:${encoded}`;
  const [revision, setRevision] = useState(0);
  const [state, setState] = useState<{
    scope: string;
    data: CommandResult<C> | null;
    error: string | null;
  }>({ scope, data: null, error: null });
  useFocusEffect(
    useCallback(() => {
      if (!snapshot || encoded === 'null') return;
      let active = true;
      void store.driver
        .request<C>(JSON.parse(encoded) as C)
        .then((data) => {
          if (active) setState({ scope, data, error: null });
        })
        .catch((error: unknown) => {
          if (active)
            setState((previous) => ({
              scope,
              data: previous.scope === scope ? previous.data : null,
              error:
                error instanceof Error
                  ? error.message
                  : 'Não foi possível carregar. Tente novamente.',
            }));
        });
      return () => {
        active = false;
      };
      // A retry deliberately restarts the focused read without changing its command.
      // oxlint-disable-next-line react/exhaustive-deps
    }, [store, snapshot, encoded, scope, revision]),
  );
  return {
    data: state.scope === scope ? state.data : null,
    error: state.scope === scope ? state.error : null,
    retry: () => setRevision((value) => value + 1),
  };
}
