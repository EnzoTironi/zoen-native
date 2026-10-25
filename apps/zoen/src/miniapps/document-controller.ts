/** Navigation awaits the current document's acknowledged save before retiring its frame. */
export type MiniappDocumentController = Readonly<{
  /** Present only for an editor with qualified same-Session external refresh. */
  observedHead?: () => Promise<Readonly<{ itemId: string; version: number; sha256: string }>>;
  flush: (options?: { retire?: boolean }) => Promise<void>;
}>;
