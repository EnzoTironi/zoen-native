import { createHash } from 'node:crypto';
import { deflateSync } from 'node:zlib';
import type { SceneToolReview } from './generated/SceneToolReview';
export const sha = (s: string | Uint8Array) => createHash('sha256').update(s).digest('hex');
const crc = (bytes: Uint8Array) => {
  let n = 0xffffffff;
  for (const b of bytes) {
    n ^= b;
    for (let i = 0; i < 8; i++) n = (n >>> 1) ^ (n & 1 ? 0xedb88320 : 0);
  }
  return (n ^ 0xffffffff) >>> 0;
};
/** Genuine tiny PNG, so transport tests cannot pass arbitrary strings as pictures. */
export function png(color: readonly number[]) {
  const chunk = (type: string, bytes: Uint8Array) => {
    const data = Buffer.concat([Buffer.from(type), bytes]);
    const size = Buffer.alloc(4);
    size.writeUInt32BE(bytes.length);
    const checksum = Buffer.alloc(4);
    checksum.writeUInt32BE(crc(data));
    return Buffer.concat([size, data, checksum]);
  };
  const head = Buffer.alloc(13);
  head.writeUInt32BE(1, 0);
  head.writeUInt32BE(1, 4);
  head[8] = 8;
  head[9] = 6;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk('IHDR', head),
    chunk('IDAT', deflateSync(Buffer.from([0, ...color]))),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}
export function drawingScene(color: string) {
  return JSON.stringify({
    type: 'excalidraw',
    version: 2,
    elements: [
      {
        id: 'rectangle',
        type: 'rectangle',
        x: 0,
        y: 0,
        width: 120,
        height: 80,
        strokeColor: color,
        backgroundColor: color,
        fillStyle: 'solid',
        strokeWidth: 1,
        roughness: 0,
        seed: 1,
        version: 1,
        isDeleted: false,
      },
    ],
    appState: { viewBackgroundColor: '#ffffff' },
    files: {},
  });
}
export function drawingReview(space = 'space', item = 'item'): SceneToolReview {
  const before_scene_json = drawingScene('#0000ff'),
    after_scene_json = drawingScene('#ff0000');
  return {
    request_id: 'request',
    content_hash: 'cd'.repeat(32),
    artifact_id: item,
    space_id: space,
    resource_uri: `ui://zoen/package/file/1/${'ef'.repeat(32)}`,
    title: 'Drawing',
    version: 1,
    before_scene_json,
    before_sha256: sha(before_scene_json),
    after_scene_json,
    after_sha256: sha(after_scene_json),
  };
}
export function drawingReply(args: Record<string, unknown>) {
  const image = (bytes: Uint8Array, sceneSha256: unknown) => ({
    mimeType: 'image/png',
    width: 1,
    height: 1,
    dataBase64: Buffer.from(bytes).toString('base64'),
    imageSha256: sha(bytes),
    sceneSha256,
  });
  return {
    content: [],
    structuredContent: {
      nonce: args.nonce,
      artifactId: args.artifactId,
      requestId: args.requestId,
      contentHash: args.contentHash,
      resourceUri: args.resourceUri,
      baseVersion: args.baseVersion,
      renderer: 'excalidraw@0.18.0',
      before: image(png([0, 0, 255, 255]), args.beforeSha256),
      after: image(png([255, 0, 0, 255]), args.afterSha256),
    },
  };
}
