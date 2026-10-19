import assert from 'node:assert/strict';
import test from 'node:test';
import { build } from 'esbuild';

const source = 'file:///app/Library/Caches/ImagePicker/original.heic';
const output = 'file:///app/Library/Caches/ImageManipulator/profile.jpg';
type Fixture = {
  source: string;
  files: Set<string>;
  deleted: string[];
  released: string[];
  canceled?: boolean;
  failure?: 'first-render' | 'second-render' | 'save' | 'source-cleanup';
  failedCleanup?: boolean;
  contexts?: number;
};
const globals = globalThis as typeof globalThis & { __zoenProfilePickerFixture?: Fixture };
const result = await build({
  entryPoints: [new URL('../profile/picker.ts', import.meta.url).pathname],
  bundle: true,
  platform: 'node',
  format: 'esm',
  write: false,
  plugins: [
    {
      name: 'synthetic-profile-sdk',
      setup(plugin) {
        plugin.onResolve(
          { filter: /^expo-(image-picker|image-manipulator|file-system)$/ },
          (args) => ({ path: args.path, namespace: 'fixture' }),
        );
        plugin.onLoad({ filter: /.*/, namespace: 'fixture' }, (args) => ({
          loader: 'js',
          contents:
            args.path === 'expo-image-picker'
              ? `
          export async function launchImageLibraryAsync() {
            const f = globalThis.__zoenProfilePickerFixture;
            return f.canceled ? { canceled: true, assets: null } : { canceled: false, assets: [{ uri: f.source }] };
          }
        `
              : args.path === 'expo-file-system'
                ? `
          export const Paths = { cache: { uri: 'file:///app/Library/Caches/' } };
          export class File {
            constructor(uri) { this.uri = uri; }
            get exists() { return globalThis.__zoenProfilePickerFixture.files.has(this.uri); }
            delete() {
              const f = globalThis.__zoenProfilePickerFixture;
              if (this.uri === f.source && f.failure === 'source-cleanup' && !f.failedCleanup) {
                f.failedCleanup = true; throw new Error('source-cleanup');
              }
              f.deleted.push(this.uri); f.files.delete(this.uri);
            }
          }
        `
                : `
          export const SaveFormat = { JPEG: 'jpeg' };
          export const ImageManipulator = {
            manipulate() {
              const f = globalThis.__zoenProfilePickerFixture;
              const which = (f.contexts = (f.contexts ?? 0) + 1) === 1 ? 'first' : 'second';
              return {
                crop() { return this; },
                release() { globalThis.__zoenProfilePickerFixture.released.push(which + '-context'); },
                async renderAsync() {
                  const f = globalThis.__zoenProfilePickerFixture;
                  if (f.failure === which + '-render') throw new Error(f.failure);
                  return {
                    width: 900, height: 700,
                    release() { f.released.push(which + '-image'); },
                    async saveAsync() {
                      if (f.failure === 'save') throw new Error('save');
                      f.files.add('${output}'); return { uri: '${output}' };
                    },
                  };
                },
              };
            },
          };
        `,
        }));
      },
    },
  ],
});
const picker = await import(
  `data:text/javascript;base64,${Buffer.from(result.outputFiles[0]!.text).toString('base64')}`
);
function fixture(options: Partial<Fixture> = {}): Fixture {
  const value = { source, files: new Set([source]), deleted: [], released: [], ...options };
  globals.__zoenProfilePickerFixture = value;
  return value;
}

test('profile preparation removes the original SDK cache copy and retains only its disposable draft', async () => {
  const f = fixture();
  const draft = await picker.pickProfilePhoto();
  assert.equal(draft.uri, output);
  assert.deepEqual(f.deleted, [source]);
  assert.deepEqual([...f.files], [output]);
  assert.equal(f.released.length, 4);
  draft.dispose();
  assert.equal(f.files.size, 0);
});

test('profile processing failures release partial objects and remove the original copy', async () => {
  for (const failure of ['first-render', 'second-render', 'save'] as const) {
    const f = fixture({ failure });
    await assert.rejects(picker.pickProfilePhoto(), new RegExp(failure));
    assert.equal(f.files.size, 0, failure);
    assert.ok(f.released.includes('first-context'), failure);
  }
});

test('a failed source deletion cannot strand a manipulated output that was never returned', async () => {
  const f = fixture({ failure: 'source-cleanup' });
  await assert.rejects(picker.pickProfilePhoto(), /source-cleanup/);
  assert.equal(f.files.size, 0);
  assert.ok(f.deleted.includes(output));
});

test('profile draft disposal never deletes a photo original outside the app cache', async () => {
  const original = 'file:///photos/original.jpg';
  const f = fixture({ source: original, files: new Set([original]) });
  const draft = await picker.pickProfilePhoto();
  draft.dispose();
  assert.deepEqual([...f.files], [original]);
  assert.equal(f.deleted.includes(original), false);
});

test('canceling the system picker creates or removes no draft', async () => {
  const f = fixture({ canceled: true, files: new Set() });
  assert.equal(await picker.pickProfilePhoto(), null);
  assert.deepEqual(f.deleted, []);
});
