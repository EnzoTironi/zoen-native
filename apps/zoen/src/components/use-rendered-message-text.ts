import { useCallback, useLayoutEffect, useRef, useState } from 'react';

export type RenderedMessageTextEvent = { revision: string; markdown: string; text: string };
export type RenderedMessageText = {
  revision: string;
  text: string | undefined;
  accept: (event: RenderedMessageTextEvent) => void;
};

/** A row-local receipt of its committed native AST, never another content store. */
export function useRenderedMessageText(
  owner: string,
  markdown: string,
  nativeMarkdown: boolean,
  shared?: RenderedMessageText,
): RenderedMessageText {
  const revision = JSON.stringify([owner, markdown, nativeMarkdown]);
  const live = useRef<string | null>(null);
  const [receipt, setReceipt] = useState<{ revision: string; text: string } | null>(null);
  useLayoutEffect(() => {
    live.current = revision;
    return () => {
      live.current = null;
    };
  }, [revision]);
  const accept = useCallback(
    (event: RenderedMessageTextEvent) => {
      if (
        !nativeMarkdown ||
        live.current !== revision ||
        event.revision !== revision ||
        event.markdown !== markdown
      )
        return;
      setReceipt((previous) =>
        live.current === revision
          ? previous?.revision === revision && previous.text === event.text
            ? previous
            : { revision, text: event.text }
          : previous,
      );
    },
    [revision, markdown, nativeMarkdown],
  );
  if (shared?.revision === revision) return shared;
  return {
    revision,
    text: nativeMarkdown ? (receipt?.revision === revision ? receipt.text : undefined) : markdown,
    accept,
  };
}
