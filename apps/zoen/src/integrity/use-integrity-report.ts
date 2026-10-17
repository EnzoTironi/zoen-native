import { useCallback, useState } from 'react';
import { useFocusEffect } from 'expo-router';
import { useClient } from '../core/provider';
import type { LogReport } from '../core/generated/LogReport';
import type { Snapshot } from '../core/types';

/** A previous success is not a verification of a changed snapshot or a new retry. */
export function useIntegrityReport(spaceId: string) {
  const { store, snapshot } = useClient();
  const [revision, setRevision] = useState(0);
  const [result, setResult] = useState<{
    snapshot: Snapshot;
    space: string;
    revision: number;
    data: LogReport | null;
    error: string | null;
  } | null>(null);
  useFocusEffect(
    useCallback(() => {
      if (!snapshot) return;
      let active = true;
      void store.driver
        .request({ op: 'verify_log', space_id: spaceId })
        .then((data) => {
          if (active) setResult({ snapshot, space: spaceId, revision, data, error: null });
        })
        .catch((reason: unknown) => {
          if (active)
            setResult({
              snapshot,
              space: spaceId,
              revision,
              data: null,
              error:
                reason instanceof Error
                  ? reason.message
                  : 'Não foi possível verificar este histórico.',
            });
        });
      return () => {
        active = false;
      };
    }, [snapshot, spaceId, store, revision]),
  );
  const current =
    result?.snapshot === snapshot && result.space === spaceId && result.revision === revision
      ? result
      : null;
  return {
    data: current?.data ?? null,
    error: current?.error ?? null,
    retry: () => setRevision((value) => value + 1),
  };
}
