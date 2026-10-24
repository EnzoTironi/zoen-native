/** Trusted build-time format parameters; the document/guest cannot widen them. */
export type BinaryEditorId = 'deckcraft' | 'gridcraft';
export type BinaryExportFormat = 'pptx' | 'pdf' | 'xlsx';
export const DECKCRAFT_ENGINE_REVISION = 'a31b990d32f2c8562f1588562f922960c3fde48b';
export const GRIDCRAFT_ENGINE_REVISION = 'b8771d51fb4da60bd48c13cf313d6d6014dc5e66';
const formats = Object.freeze({
  deckcraft: Object.freeze({
    id: 'deckcraft' as const,
    messages: 'decks' as const,
    resourceUri: 'ui://zoen/deckcraft/index.html',
    engineRevision: DECKCRAFT_ENGINE_REVISION,
    maxDocumentBytes: 8 * 1024 * 1024,
    exports: Object.freeze(['pptx', 'pdf'] as const),
  }),
  gridcraft: Object.freeze({
    id: 'gridcraft' as const,
    messages: 'workbooks' as const,
    resourceUri: 'ui://zoen/gridcraft/index.html',
    engineRevision: GRIDCRAFT_ENGINE_REVISION,
    maxDocumentBytes: 8 * 1024 * 1024,
    exports: Object.freeze(['xlsx'] as const),
  }),
});
export function binaryEditorFormat(id: unknown = 'deckcraft') {
  if (id !== 'deckcraft' && id !== 'gridcraft') throw Error('Unsupported binary App format.');
  return formats[id];
}
export function binaryExportAllowed(
  id: BinaryEditorId,
  value: unknown,
): value is BinaryExportFormat {
  return (
    typeof value === 'string' &&
    (binaryEditorFormat(id).exports as readonly string[]).includes(value)
  );
}
