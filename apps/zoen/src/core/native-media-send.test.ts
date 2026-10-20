import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild';
import type { CoreDriver } from './types.ts';
import type { SendFile } from '../media/types.ts';

// Exercise the real native adapter; only its OS resources are replaced.
const ports: Record<string, string> = {
  '../../modules/zoen-core/src/ZoenCoreModule': `
    export const requests = [];
    export const previews = [];
    export const canceled = [];
    let failure = false;
    export function failNextSend() { failure = true; }
    export default {
      async loadMediaDraft() {},
      async previewMediaDraftAsset(request, token) { previews.push({request, token}); },
      async cancelMediaDraftPreview(token) { canceled.push(token); },
      async sendFile(request) {
        requests.push(request);
        if (failure) { failure = false; throw Error('Synthetic interrupted reply'); }
        return JSON.stringify({itemId: 'qa_file', messageId: 'qa_message'});
      },
    };
  `,
  '../profile/native-driver': 'export const createNativeProfilePhotoDriver = () => undefined;',
  'react-native': 'export const Platform = { OS: "ios" };',
  '../i18n/index.ts': 'export const locale = "en";',
};
const output = await build({
  stdin: {
    contents: `export {createDriver} from './driver.ts';
      export {requests, previews, canceled, failNextSend} from '../../modules/zoen-core/src/ZoenCoreModule';`,
    resolveDir: new URL('.', import.meta.url).pathname,
    loader: 'ts',
  },
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false,
  plugins: [
    {
      name: 'native-media-resources',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          path in ports ? { path, namespace: 'native-port' } : undefined,
        );
        builder.onLoad({ filter: /.*/, namespace: 'native-port' }, ({ path }) => ({
          contents: ports[path],
        }));
      },
    },
  ],
});
const native = (await import(
  `data:text/javascript;base64,${Buffer.from(output.outputFiles[0]!.text).toString('base64')}`
)) as {
  createDriver(): CoreDriver;
  requests: string[];
  previews: { request: string; token: string }[];
  canceled: string[];
  failNextSend(): void;
};
const selection: SendFile = {
  uri: 'file:///synthetic/picked.pdf',
  name: 'picked.pdf',
  mime: 'application/pdf',
  spaceId: 'qa_space',
  replyTo: 'qa_root',
  thread: true,
  mutationId: 'qa_fixed_selection',
};

test('native file sends preserve caption bytes and the same retry intent after an interrupted reply', async () => {
  native.requests.length = 0;
  const media = native.createDriver().media!;
  const captioned = { ...selection, caption: '  Página 2 🗂️\nTotal: 161.  ' };
  native.failNextSend();
  await assert.rejects(media.sendFile(captioned), /interrupted reply/);
  assert.deepEqual(await media.sendFile(captioned), {
    itemId: 'qa_file',
    messageId: 'qa_message',
  });
  assert.equal(native.requests.length, 2);
  assert.equal(native.requests[0], native.requests[1]);
  assert.deepEqual(JSON.parse(native.requests[0]!), captioned);
});

test('native file sends without a caption retain blank text and their original file target', async () => {
  native.requests.length = 0;
  await native.createDriver().media!.sendFile(selection);
  assert.deepEqual(JSON.parse(native.requests[0]!), { ...selection, caption: '' });
});

test('native draft review forwards canonical scope/revision/asset and cancellation token without a URI', async () => {
  native.previews.length = 0;
  native.canceled.length = 0;
  const drafts = native.createDriver().media!.drafts!;
  const scope = {
    expected_identity_id: 'owner',
    expected_device_id: 'device',
    space_id: 'chat',
    thread_root_id: 'root',
  };
  await drafts.preview!(scope, 'revision-7', 'selected-asset', 'presentation-token');
  assert.deepEqual(native.previews, [
    {
      request: JSON.stringify({
        scope,
        expected_revision: 'revision-7',
        asset_id: 'selected-asset',
      }),
      token: 'presentation-token',
    },
  ]);
  assert.equal(native.previews[0]!.request.includes('uri'), false);
  await drafts.cancelPreview!('presentation-token');
  assert.deepEqual(native.canceled, ['presentation-token']);
});
