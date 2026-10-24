import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild';
import type { CoreDriver } from './types';

// Compile the production driver; replace only OS resources/opaque native calls.
async function fixture(platform: string, available: boolean) {
  const ports: Record<string, string> = {
    '../../modules/zoen-core/src/ZoenCoreModule': `
      export const calls=[];
      export default { ${available ? "async readContactFile(request) { calls.push(JSON.parse(request)); return JSON.stringify({name:'Ana', phones:[]}); }," : ''}
        releaseLocalFile() { throw Error('read must not release'); },
        sendFile() { throw Error('read must not send'); }
      };`,
    '../profile/native-driver': 'export const createNativeProfilePhotoDriver = () => undefined;',
    'react-native': `export const Platform={OS:${JSON.stringify(platform)}};`,
    '../i18n/index.ts': 'export const locale="en";',
    'expo-crypto': 'export const randomUUID=()=>"synthetic";',
  };
  const output = await build({
    stdin: {
      contents:
        "export {createDriver} from './driver.ts'; export {calls} from '../../modules/zoen-core/src/ZoenCoreModule';",
      resolveDir: new URL('.', import.meta.url).pathname,
      loader: 'ts',
    },
    bundle: true,
    platform: 'node',
    format: 'esm',
    write: false,
    plugins: [
      {
        name: 'contact-native-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) =>
            path in ports ? { path, namespace: 'port' } : undefined,
          );
          builder.onLoad({ filter: /.*/, namespace: 'port' }, ({ path }) => ({
            contents: ports[path],
          }));
        },
      },
    ],
  });
  return (await import(
    `data:text/javascript;base64,${Buffer.from(output.outputFiles[0]!.text).toString('base64')}`
  )) as { createDriver(): CoreDriver; calls: unknown[] };
}

test('real iOS driver advertises native contact reads only when supported and sends the exact authorized reference', async () => {
  const f = await fixture('ios', true);
  const file = {
    uri: 'file:///synthetic/contact.vcf',
    itemId: 'item',
    version: 3,
    info: {
      name: 'contact.vcf',
      path: '',
      mime: 'text/vcard',
      bytes: '180',
      sha256: 'a'.repeat(64),
      ready: true,
      chunks: 1,
      chunks_here: 1,
      has_thumbnail: false,
    },
  };
  assert.deepEqual(await f.createDriver().media!.readContact!(file), { name: 'Ana', phones: [] });
  assert.deepEqual(f.calls, [
    { uri: file.uri, itemId: 'item', version: 3, sha256: file.info.sha256 },
  ]);
});

test('older native binaries and Android/Web retain existing file behavior without advertising contact parsing', async () => {
  for (const [platform, available] of [
    ['ios', false],
    ['android', true],
    ['web', true],
  ] as const) {
    const f = await fixture(platform, available);
    const driver = f.createDriver();
    if (platform === 'ios') assert.ok(driver.media);
    else
      assert.equal(driver.media, undefined, 'existing non-iOS native-driver surface is unchanged');
    assert.equal(driver.media?.readContact, undefined);
    assert.deepEqual(f.calls, []);
  }
});
