/** Trusted build-time format parameters; the document/guest cannot widen them. */
export type BinaryEditorId = 'deckcraft' | 'gridcraft' | 'wordcraft' | 'photocraft';
export type BinaryExportFormat = 'pptx' | 'pdf' | 'xlsx' | 'docx' | 'pcraft' | 'png';
export const DECKCRAFT_ENGINE_REVISION = 'a31b990d32f2c8562f1588562f922960c3fde48b';
export const GRIDCRAFT_ENGINE_REVISION = 'b8771d51fb4da60bd48c13cf313d6d6014dc5e66';
export const WORDCRAFT_ENGINE_REVISION = 'd1b0bfc3f9dd83126b162a42b667facecb003b7f';
export const PHOTOCRAFT_ENGINE_REVISION = '9b9643764f3fb77d9f0e48d7510983ae50621e11';
const formats = Object.freeze({
  deckcraft: Object.freeze({
    id: 'deckcraft' as const,
    liveReview: false,
    messages: 'decks' as const,
    resourceUri: 'ui://zoen/deckcraft/index.html',
    engineRevision: DECKCRAFT_ENGINE_REVISION,
    maxDocumentBytes: 8 * 1024 * 1024,
    exports: Object.freeze(['pptx', 'pdf'] as const),
  }),
  wordcraft: Object.freeze({
    id: 'wordcraft' as const,
    liveReview: true,
    messages: 'wordDocuments' as const,
    resourceUri: 'ui://zoen/wordcraft/index.html',
    engineRevision: WORDCRAFT_ENGINE_REVISION,
    maxDocumentBytes: 512 * 1024,
    exports: Object.freeze(['docx', 'pdf'] as const),
  }),
  gridcraft: Object.freeze({
    id: 'gridcraft' as const,
    liveReview: false,
    messages: 'workbooks' as const,
    resourceUri: 'ui://zoen/gridcraft/index.html',
    engineRevision: GRIDCRAFT_ENGINE_REVISION,
    maxDocumentBytes: 8 * 1024 * 1024,
    exports: Object.freeze(['xlsx'] as const),
  }),
  photocraft: Object.freeze({
    id: 'photocraft' as const,
    liveReview: true,
    messages: 'wordDocuments' as const,
    resourceUri: 'ui://zoen/photocraft/index.html',
    engineRevision: PHOTOCRAFT_ENGINE_REVISION,
    maxDocumentBytes: 8 * 1024 * 1024,
    exports: Object.freeze(['pcraft', 'png'] as const),
  }),
});
export function isBinaryEditorId(id: unknown): id is BinaryEditorId {
  return id === 'deckcraft' || id === 'gridcraft' || id === 'wordcraft' || id === 'photocraft';
}
export function binaryEditorFormat(id: unknown = 'deckcraft') {
  if (!isBinaryEditorId(id)) throw Error('Unsupported binary App format.');
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

/** Closed OS share types. Export authorization still belongs to the selected editor/head. */
export function binaryExportFileType(format: BinaryExportFormat) {
  switch (format) {
    case 'pcraft':
      return { mimeType: 'application/x-photocraft' };
    case 'png':
      return { mimeType: 'image/png', UTI: 'public.png' };
    case 'docx':
      return {
        mimeType: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
        UTI: 'org.openxmlformats.wordprocessingml.document',
      };
    case 'xlsx':
      return { mimeType: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet' };
    case 'pdf':
      return { mimeType: 'application/pdf', UTI: 'com.adobe.pdf' };
    case 'pptx':
      return {
        mimeType: 'application/vnd.openxmlformats-officedocument.presentationml.presentation',
        UTI: 'org.openxmlformats.presentationml.presentation',
      };
    default:
      throw Error('Unsupported binary export format.');
  }
}
