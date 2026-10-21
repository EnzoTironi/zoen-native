import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild';
import type { AttachmentAsset } from '../media/attachment-draft.ts';

type PickerFixture = {
  cameras: Record<string, unknown>[];
  galleries: Record<string, unknown>[];
  documents: Record<string, unknown>[];
  permission(): Promise<{ granted: boolean }>;
};
const globals = globalThis as typeof globalThis & { __zoenAttachmentPicker?: PickerFixture };
const output = await build({
  entryPoints: [new URL('../media/attachment-picker.ts', import.meta.url).pathname],
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false,
  plugins: [
    {
      name: 'attachment-picker-sdk-ports',
      setup(builder) {
        builder.onResolve(
          {
            filter:
              /^(react-native|expo-file-system|expo-document-picker|expo-image-picker|\.\.\/i18n)$/,
          },
          ({ path }) => ({ path, namespace: 'picker-port' }),
        );
        builder.onLoad({ filter: /.*/, namespace: 'picker-port' }, ({ path }) => ({
          loader: 'js',
          contents: (
            {
              'react-native': 'export const Platform = {OS:"ios"};',
              'expo-file-system':
                'export class File { constructor(uri) { this.uri=uri; this.exists=true; this.size=42; } }',
              'expo-document-picker':
                'export const getDocumentAsync = async options => { globalThis.__zoenAttachmentPicker.documents.push(options); return {canceled:true}; };',
              '../i18n': 'export const t = key => key;',
              'expo-image-picker': `
            export const requestCameraPermissionsAsync = () => globalThis.__zoenAttachmentPicker.permission();
            export async function launchCameraAsync(options) {
              globalThis.__zoenAttachmentPicker.cameras.push(options);
              return {canceled:false,assets:[{uri:'file:///cache/captured.mov',type:'video'}]};
            }
            export async function launchImageLibraryAsync(options) {
              globalThis.__zoenAttachmentPicker.galleries.push(options);
              return {canceled:true};
            }
          `,
            } as Record<string, string>
          )[path]!,
        }));
      },
    },
  ],
});
const { pickAttachmentAssets } = (await import(
  `data:text/javascript;base64,${Buffer.from(output.outputFiles[0]!.text).toString('base64')}`
)) as {
  pickAttachmentAssets(
    kind: string,
    remaining: number,
    current: () => boolean,
    filter?: { mime: 'audio/*' },
  ): Promise<AttachmentAsset[]>;
};
function fixture() {
  const value: PickerFixture = {
    cameras: [],
    galleries: [],
    documents: [],
    permission: async () => ({ granted: true }),
  };
  globals.__zoenAttachmentPicker = value;
  return value;
}

test('iOS camera offers photos and videos and returns the owned capture with video metadata', async () => {
  const f = fixture();
  const assets = await pickAttachmentAssets('camera', 4, () => true);
  assert.deepEqual(f.cameras, [{ mediaTypes: ['images', 'videos'], quality: 1, exif: false }]);
  assert.deepEqual(assets, [
    {
      uri: 'file:///cache/captured.mov',
      name: 'video.mov',
      mime: 'video/quicktime',
      size: 42,
    },
  ]);
});

test('camera permission completion cannot launch capture for a departed composer', async () => {
  const f = fixture();
  let current = true;
  f.permission = async () => {
    current = false;
    return { granted: true };
  };
  assert.deepEqual(await pickAttachmentAssets('camera', 1, () => current), []);
  assert.deepEqual(f.cameras, []);
});

test('gallery retains ordered multi-selection and the remaining batch limit', async () => {
  const f = fixture();
  await pickAttachmentAssets('photo', 3, () => true);
  assert.deepEqual(f.galleries, [
    {
      mediaTypes: ['images', 'videos'],
      quality: 1,
      allowsMultipleSelection: true,
      selectionLimit: 3,
      orderedSelection: true,
      exif: false,
    },
  ]);
});

test('Audio Browse applies the SDK58 MIME filter without invoking camera or gallery', async () => {
  const f = fixture();
  await pickAttachmentAssets('document', 3, () => true, { mime: 'audio/*' });
  assert.deepEqual(f.documents, [
    { copyToCacheDirectory: true, multiple: true, type: 'audio/*', base64: false },
  ]);
  assert.deepEqual(f.cameras, []);
  assert.deepEqual(f.galleries, []);
});
