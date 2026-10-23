import { useCallback, useState } from 'react';
import { useFocusEffect } from 'expo-router';
import { useClient } from './provider';
import type { Command, CommandResult } from './types';

/** Focus-scoped projections; old routes and accounts cannot publish late results. */
export function useCoreQuery<C extends Command>(command: C | null, freshness = '') {
  const { store, snapshot, mode } = useClient();
  const encoded = JSON.stringify(command);
  const scope = `${mode}:${snapshot?.account?.identity_id ?? ''}:${encoded}:${freshness}`;
  const [revision, setRevision] = useState(0);
  const [state, setState] = useState<{
    scope: string;
    data: CommandResult<C> | null;
    error: string | null;
    loading: boolean;
  }>({ scope, data: null, error: null, loading: true });
  useFocusEffect(
    useCallback(() => {
      if (!snapshot || encoded === 'null') return;
      let active = true;
      setState((previous) => ({
        scope,
        data: previous.scope === scope ? previous.data : null,
        error: null,
        loading: true,
      }));
      void store.driver
        .request<C>(JSON.parse(encoded) as C)
        .then((data) => {
          if (active) setState({ scope, data, error: null, loading: false });
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
              loading: false,
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
    loading: state.scope !== scope || state.loading,
    retry: () => setRevision((value) => value + 1),
  };
}
