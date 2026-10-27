import { createContext, use, useLayoutEffect, useMemo, type ReactNode } from 'react';
import { View } from 'react-native';
import { ConversationRevealMeasurement } from '../core/conversation-reveal-measurement';
import type { ConversationContentLayout } from '../core/conversation-reveal-content';

const RevealAnchor = createContext<Readonly<{
  id: string;
  onLayout: ConversationRevealMeasurement['anchor'];
  onTextLayout: ConversationRevealMeasurement['text'];
}> | null>(null);

export function useConversationRevealAnchor(id: string) {
  const anchor = use(RevealAnchor);
  return anchor?.id === id ? anchor : null;
}

export function ConversationRevealContentView({
  id,
  query,
  expectsText,
  onLayout,
  children,
}: {
  id: string;
  query: string;
  expectsText: boolean;
  onLayout: (id: string, layout: ConversationContentLayout) => void;
  children: ReactNode;
}) {
  const measurement = useMemo(
    () => new ConversationRevealMeasurement(expectsText, (layout) => onLayout(id, layout)),
    [id, onLayout, expectsText],
  );
  useLayoutEffect(() => {
    measurement.activate();
    return () => measurement.retire();
  }, [measurement]);
  useLayoutEffect(() => measurement.setQuery(query), [measurement, query]);
  const context = useMemo(
    () => ({
      id,
      onLayout: measurement.anchor,
      onTextLayout: measurement.text,
    }),
    [id, measurement],
  );
  return (
    <RevealAnchor value={context}>
      <View collapsable={false} onLayout={measurement.body}>
        {children}
      </View>
    </RevealAnchor>
  );
}
