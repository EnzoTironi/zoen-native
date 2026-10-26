import type { ClientStore } from '../core/store';
import type { SceneToolReview } from '../core/generated/SceneToolReview';
import { accountScope, accountScopeKey } from '../core/account-scope.ts';
import { homeSpacesFor } from '../home/access.ts';
import { decode64, hash } from './canonical-scene-codec.ts';
import type { CanonicalResourcePin } from './canonical-resource';
import type { DrawingReviewImage, DrawingReviewPort } from './drawing-review-types';

const digest = (v: unknown): v is string => typeof v === 'string' && /^[a-f0-9]{64}$/.test(v);
const record = (v: unknown): v is Record<string, unknown> =>
  !!v && typeof v === 'object' && !Array.isArray(v);
const textHash = (v: string) => {
  const bytes = new TextEncoder().encode(v);
  if (bytes.length > 32768) throw Error('drawing_review_changed');
  return hash(bytes);
};
const PNG_BYTES = 128 * 1024;
function image(value: unknown, sceneSha: string): DrawingReviewImage {
  if (
    !record(value) ||
    value.mimeType !== 'image/png' ||
    value.sceneSha256 !== sceneSha ||
    !digest(value.imageSha256) ||
    typeof value.dataBase64 !== 'string'
  )
    throw Error('drawing_review_invalid_image');
  const bytes = decode64(value.dataBase64);
  if (
    bytes.length < 45 ||
    bytes.length > PNG_BYTES ||
    hash(bytes) !== value.imageSha256 ||
    ![137, 80, 78, 71, 13, 10, 26, 10].every((b, i) => bytes[i] === b) ||
    String.fromCharCode(...bytes.subarray(12, 16)) !== 'IHDR' ||
    ![0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130].every(
      (b, i) => bytes[bytes.length - 12 + i] === b,
    )
  )
    throw Error('drawing_review_invalid_image');
  const data = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const width = data.getUint32(16),
    height = data.getUint32(20);
  if (
    !width ||
    !height ||
    width > 896 ||
    height > 896 ||
    value.width !== width ||
    value.height !== height
  )
    throw Error('drawing_review_invalid_image');
  return Object.freeze({
    uri: `data:image/png;base64,${value.dataBase64}`,
    width,
    height,
    sha256: value.imageSha256,
  });
}
function valid(
  review: SceneToolReview,
  id: string,
  sha: string,
  pin: CanonicalResourcePin,
  space: string,
) {
  return (
    review.request_id === id &&
    review.content_hash === sha &&
    review.artifact_id === pin.itemId &&
    review.resource_uri === pin.resourceUri &&
    review.space_id === space &&
    Number.isSafeInteger(review.version) &&
    review.version > 0 &&
    digest(review.before_sha256) &&
    digest(review.after_sha256) &&
    typeof review.before_scene_json === 'string' &&
    typeof review.after_scene_json === 'string' &&
    review.before_scene_json.length <= 32768 &&
    review.after_scene_json.length <= 32768 &&
    textHash(review.before_scene_json) === review.before_sha256 &&
    textHash(review.after_scene_json) === review.after_sha256
  );
}

function fingerprint(v: SceneToolReview) {
  return JSON.stringify([
    v.request_id,
    v.content_hash,
    v.artifact_id,
    v.space_id,
    v.resource_uri,
    v.title,
    v.version,
    v.before_sha256,
    v.before_scene_json,
    v.after_sha256,
    v.after_scene_json,
  ]);
}

/** Only signed Core bytes enter the renderer; every late result is reauthorized. */
export function drawingReviewPort(options: {
  store: ClientStore;
  pin: CanonicalResourcePin;
  spaceId: string;
  key: string;
  id: () => string;
  check: () => void;
  call: (
    name: string,
    args: Record<string, unknown>,
  ) => Promise<{ isError?: boolean; structuredContent?: unknown }>;
}) {
  const { store, pin } = options;
  const owner = accountScopeKey(accountScope(store));
  let active = true,
    available = false;
  let serial = Promise.resolve();
  const listeners = new Set<() => void>();
  const check = () => {
    options.check();
    if (
      !active ||
      !owner ||
      accountScopeKey(accountScope(store)) !== owner ||
      !homeSpacesFor(store.getSnapshot()).has(options.spaceId)
    )
      throw Error('drawing_review_owner_changed');
  };
  const publish = (next: boolean) => {
    if (available === next) return;
    available = next;
    for (const listener of listeners) listener();
  };
  const render = async (requestId: string, contentHash: string) => {
    check();
    if (!available || !/^[A-Za-z0-9_-]{1,128}$/.test(requestId) || !digest(contentHash))
      throw Error('drawing_review_unavailable');
    const scope = accountScope(store);
    if (!scope) throw Error('drawing_review_owner_changed');
    const input = {
      expected_identity_id: scope.identity,
      expected_device_id: scope.device,
      space_id: options.spaceId,
      item_id: pin.itemId,
      expected_resource_uri: pin.resourceUri,
      request_id: requestId,
      expected_hash: contentHash,
    };
    const before = await store.driver.request({ op: 'scene_tool_review', input });
    check();
    if (!valid(before, requestId, contentHash, pin, options.spaceId))
      throw Error('drawing_review_changed');
    if (!available) throw Error('drawing_review_unavailable');
    const nonce = options.id();
    const result = await options.call('render_scene_review', {
      nonce,
      artifactId: pin.itemId,
      requestId,
      contentHash,
      resourceUri: pin.resourceUri,
      baseVersion: before.version,
      beforeSceneJson: before.before_scene_json,
      beforeSha256: before.before_sha256,
      afterSceneJson: before.after_scene_json,
      afterSha256: before.after_sha256,
    });
    check();
    const value = result.structuredContent;
    if (
      result.isError ||
      !record(value) ||
      value.nonce !== nonce ||
      value.artifactId !== pin.itemId ||
      value.requestId !== requestId ||
      value.contentHash !== contentHash ||
      value.resourceUri !== pin.resourceUri ||
      value.baseVersion !== before.version ||
      value.renderer !== 'excalidraw@0.18.0'
    )
      throw Error('drawing_review_changed');
    const images = {
      before: image(value.before, before.before_sha256),
      after: image(value.after, before.after_sha256),
    };
    const after = await store.driver.request({ op: 'scene_tool_review', input });
    check();
    if (
      !available ||
      !valid(after, requestId, contentHash, pin, options.spaceId) ||
      fingerprint(before) !== fingerprint(after)
    )
      throw Error('drawing_review_changed');
    return Object.freeze({
      ...images,
      review: before,
      key: `${options.key}:${requestId}:${contentHash}:${nonce}`,
    });
  };
  const port: DrawingReviewPort = {
    key: options.key,
    getSnapshot: () => available,
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    render: (id, sha) => {
      const task = serial.then(() => render(id, sha));
      serial = task.then(
        () => {},
        () => {},
      );
      return task;
    },
  };
  return {
    ...port,
    observe(value: unknown) {
      let usable = false;
      try {
        check();
        const state =
          record(value) && record(value.zoen) && record(value.zoen.sceneSave)
            ? value.zoen.sceneSave
            : null;
        usable =
          !!state &&
          state.itemId === pin.itemId &&
          state.ready === true &&
          state.reviewPreview === 1;
      } catch {
        /* Preview availability cannot change a document save acknowledgment. */
      }
      publish(usable);
    },
    dispose() {
      active = false;
      publish(false);
      listeners.clear();
    },
  };
}
