import { decode64, hash, CANONICAL_SCENE_CHUNK_BYTES as CHUNK } from './canonical-scene.ts';
import { binaryEditorFormat, type BinaryEditorId } from './binary-formats.ts';
import { digest, positive, token, scope } from './deckcraft-document-wire.ts';

/** One bounded canonical download for initial open and same-Session head refresh. */
export async function loadBinaryDocument(
  editor: BinaryEditorId | undefined,
  itemId: string,
  tool: (name: string, args: Record<string, unknown>) => Promise<Record<string, unknown>>,
) {
  const read = await tool('app_binary_read', {});
  if (
    !positive(read.version) ||
    !positive(read.currentVersion) ||
    read.currentVersion < read.version ||
    !digest(read.sha256) ||
    !positive(read.bytes) ||
    read.bytes > binaryEditorFormat(editor).maxDocumentBytes ||
    read.chunkBytes !== CHUNK ||
    !token(read.transferId) ||
    read.engineRevision !== binaryEditorFormat(editor).engineRevision
  )
    throw new Error('Invalid canonical document.');
  const binding = scope(read.scope, itemId);
  const bytes = new Uint8Array(read.bytes);
  try {
    for (let index = 0; index < Math.ceil(bytes.length / CHUNK); index++) {
      const chunk = await tool('app_binary_read_chunk', { transferId: read.transferId, index });
      if (chunk.index !== index) throw new Error('Invalid document chunk order.');
      const decoded = decode64(chunk.data);
      if (decoded.length !== Math.min(CHUNK, bytes.length - index * CHUNK))
        throw new Error('Invalid document chunk length.');
      bytes.set(decoded, index * CHUNK);
    }
    if (hash(bytes) !== read.sha256) throw new Error('The document hash does not match.');
    return { read, binding, bytes };
  } catch (error) {
    bytes.fill(0);
    throw error;
  }
}
