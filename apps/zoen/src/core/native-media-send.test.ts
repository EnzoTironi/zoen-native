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
    export const edits = [];
    export const recoveryRequests = [];
    let failure = false;
    export function failNextSend() { failure = true; }
    export const nativeModule = {
      failedMediaRecoveryVersion: 1,
      async loadMediaDraft() {},
      async request(request) {
        recoveryRequests.push(JSON.parse(request));
        return JSON.stringify({revision:'recovered',content_json:'null',assets:[{id:'a',name:'a.pdf',mime:'application/pdf',size:'1048576',sha256:'a'.repeat(64)}],attempted:false});
      },
      async previewMediaDraftAsset(request, token) { previews.push({request, token}); },
      async editMediaDraftAsset(request, token) {
        edits.push({request, token});
        return JSON.stringify({revision:'edited', content_json:'null', assets:[], attempted:false});
      },
      async cancelMediaDraftPreview(token) { canceled.push(token); },
      async sendFile(request) {
        requests.push(request);
        if (failure) { failure = false; throw Error('Synthetic interrupted reply'); }
        return JSON.stringify({itemId: 'qa_file', messageId: 'qa_message'});
      },
    };
    export default nativeModule;
  `,
  '../profile/native-driver': 'export const createNativeProfilePhotoDriver = () => undefined;',
  'react-native': 'export const Platform = { OS: "ios" };',
  '../i18n/index.ts': 'export const locale = "en";',
  'expo-crypto': 'export const randomUUID = () => "00000000-0000-4000-8000-000000000042";',
};
const output = await build({
  stdin: {
    contents: `export {createDriver} from './driver.ts';
      export {requests, previews, edits, canceled, recoveryRequests, nativeModule, failNextSend} from '../../modules/zoen-core/src/ZoenCoreModule';`,
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
  edits: { request: string; token: string }[];
  recoveryRequests: unknown[];
  nativeModule: { failedMediaRecoveryVersion?: number };
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

test('native recovery adapter sends only scoped CAS identities and converts bounded integer wire sizes without Send', async () => {
  native.recoveryRequests.length = 0;
  native.requests.length = 0;
  const scope = {
    expected_identity_id: 'owner',
    expected_device_id: 'device',
    space_id: 'chat',
    thread_root_id: null,
  };
  const result = await native.createDriver().media!.drafts!.recoverFailed!(
    scope,
    'failed',
    'media-revision',
    'normal-revision',
  );
  assert.deepEqual(native.recoveryRequests, [
    {
      op: 'recover_failed_media_draft',
      scope,
      message_id: 'failed',
      expected_revision: 'media-revision',
      expected_message_revision: 'normal-revision',
    },
  ]);
  assert.equal(result.assets[0]!.size, 1048576);
  assert.equal(native.requests.length, 0);
});

test('older native binaries cannot advertise or invoke the new recovery command', () => {
  delete native.nativeModule.failedMediaRecoveryVersion;
  try {
    assert.equal(native.createDriver().media!.drafts!.recoverFailed, undefined);
  } finally {
    native.nativeModule.failedMediaRecoveryVersion = 1;
  }
});

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

test('native edited review uses canonical scope and revision without exposing an input URI', async () => {
  native.edits.length = 0;
  const drafts = native.createDriver().media!.drafts!;
  const scope = {
    expected_identity_id: 'owner',
    expected_device_id: 'device',
    space_id: 'chat',
    thread_root_id: 'root',
  };
  assert.deepEqual(await drafts.edit!(scope, 'baseline', 'selected', 'presentation'), {
    revision: 'edited',
    content_json: 'null',
    assets: [],
    attempted: false,
  });
  assert.deepEqual(native.edits, [
    {
      request: JSON.stringify({ scope, expected_revision: 'baseline', asset_id: 'selected' }),
      token: 'presentation',
    },
  ]);
  assert.equal(native.edits[0]!.request.includes('uri'), false);
});
