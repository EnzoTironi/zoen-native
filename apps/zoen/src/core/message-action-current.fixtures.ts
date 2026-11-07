import assert from 'node:assert/strict';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import type * as CurrentActions from '../components/message-action-current';

const ownerBundle = await build({
  entryPoints: [new URL('../components/message-action-current.ts', import.meta.url).pathname],
  bundle: true,
  external: ['../i18n/index.ts'],
  platform: 'node',
  format: 'cjs',
  write: false,
});
const ownerModule = { exports: {} as typeof CurrentActions };
runInNewContext(ownerBundle.outputFiles[0]!.text, {
  module: ownerModule,
  exports: ownerModule.exports,
  require: (name: string) => {
    assert.equal(name, '../i18n/index.ts');
    return { t: (key: string) => key };
  },
});
export const { messageActionOwner } = ownerModule.exports;
