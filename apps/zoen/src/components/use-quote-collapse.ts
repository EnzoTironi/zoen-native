import { useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { t } from '../i18n/index.ts';
import { useQuoteLayoutTransition } from './quote-layout-context';

let nextGeneration = 1;

/** One ephemeral owner for quote presentation; the native renderer owns its AST/layout. */
export function useQuoteCollapse(
  text: string,
  id: string | undefined,
  revision: string | undefined,
  selecting: boolean,
) {
  const beginLayoutTransition = useQuoteLayoutTransition();
  const owner = useMemo(
    () => ({ text, id, revision, generation: nextGeneration++ }),
    [text, id, revision],
  );
  const [state, setState] = useState({ generation: 0, expanded: [] as number[] });
  // This is only a cheap candidate gate. The maintained AST decides which
  // actual quote blocks qualify; no quote parsing or text replacement lives here.
  const enabled = process.env.EXPO_OS === 'ios' && text.includes('>') && !selecting;
  const eventGeneration = useMemo(() => nextGeneration++, [owner, enabled]);
  const activeGeneration = useRef<number | null>(null);
  useLayoutEffect(() => {
    activeGeneration.current = enabled ? eventGeneration : null;
    if (!enabled) beginLayoutTransition?.finish(eventGeneration);
    return () => {
      beginLayoutTransition?.finish(eventGeneration);
      activeGeneration.current = null;
    };
  }, [owner, enabled, eventGeneration, beginLayoutTransition]);
  const onBlockquoteToggle = useCallback(
    (event: { nativeEvent: { index: number; generation: number } }) => {
      const { index, generation } = event.nativeEvent;
      if (
        generation !== eventGeneration ||
        activeGeneration.current !== generation ||
        !Number.isSafeInteger(index) ||
        index < 0 ||
        index > 2147483647
      )
        return;
      beginLayoutTransition?.begin(generation);
      setState((previous) => {
        if (activeGeneration.current !== generation) return previous;
        const expanded = previous.generation === owner.generation ? previous.expanded : [];
        return {
          generation: owner.generation,
          expanded: (expanded.includes(index)
            ? expanded.filter((value) => value !== index)
            : [...expanded, index]
          ).sort((a, b) => a - b),
        };
      });
    },
    [owner, eventGeneration, beginLayoutTransition],
  );
  return {
    quoteCollapse: {
      enabled,
      expanded: state.generation === owner.generation ? state.expanded : [],
      generation: eventGeneration,
      expandLabel: t('chat.expandQuote'),
      collapseLabel: t('chat.collapseQuote'),
      expandedLabel: t('chat.quoteExpanded'),
      collapsedLabel: t('chat.quoteCollapsed'),
    },
    onBlockquoteToggle,
  };
}
