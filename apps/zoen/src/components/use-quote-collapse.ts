import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { t } from '../i18n/index.ts';

let nextGeneration = 1;
type Owner = { text: string; id?: string; revision?: string; generation: number };

/** One ephemeral owner for quote presentation; the native renderer owns its AST/layout. */
export function useQuoteCollapse(
  text: string,
  id: string | undefined,
  revision: string | undefined,
  selecting: boolean,
) {
  const owner = useMemo(
    () => ({ text, id, revision, generation: nextGeneration++ }),
    [text, id, revision],
  );
  const ownerRef = useRef<Owner | null>(null);
  const [state, setState] = useState({ generation: 0, expanded: [] as number[] });
  // This is only a cheap candidate gate. The maintained AST decides which
  // actual quote blocks qualify; no quote parsing or text replacement lives here.
  const enabled = process.env.EXPO_OS === 'ios' && text.includes('>') && !selecting;
  const enabledRef = useRef(enabled);
  useLayoutEffect(() => {
    ownerRef.current = owner;
    enabledRef.current = enabled;
    return () => {
      if (ownerRef.current?.generation === owner.generation) ownerRef.current = null;
    };
  }, [owner, enabled]);
  const onBlockquoteToggle = useCallback(
    (event: { nativeEvent: { index: number; generation: number } }) => {
      const { index, generation } = event.nativeEvent;
      if (
        !enabledRef.current ||
        generation !== owner.generation ||
        ownerRef.current?.generation !== generation ||
        !Number.isSafeInteger(index) ||
        index < 0 ||
        index > 2147483647
      )
        return;
      setState((previous) => {
        if (!enabledRef.current || ownerRef.current?.generation !== generation) return previous;
        const expanded = previous.generation === generation ? previous.expanded : [];
        return {
          generation,
          expanded: (expanded.includes(index)
            ? expanded.filter((value) => value !== index)
            : [...expanded, index]
          ).sort((a, b) => a - b),
        };
      });
    },
    [owner],
  );
  return {
    quoteCollapse: {
      enabled,
      expanded: state.generation === owner.generation ? state.expanded : [],
      generation: owner.generation,
      expandLabel: t('chat.expandQuote'),
      collapseLabel: t('chat.collapseQuote'),
      expandedLabel: t('chat.quoteExpanded'),
      collapsedLabel: t('chat.quoteCollapsed'),
    },
    onBlockquoteToggle,
  };
}
