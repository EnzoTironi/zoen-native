/** Navigation awaits the current document's acknowledged save before retiring its frame. */
export type MiniappDocumentController = Readonly<{
  flush: (options?: { retire?: boolean }) => Promise<void>;
}>;
