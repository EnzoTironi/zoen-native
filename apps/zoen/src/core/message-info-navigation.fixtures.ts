import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as scope from './conversation-scope.ts';
import * as access from '../home/access.ts';
import type { messageInfoAction } from '../components/message-info-navigation';
const compiled = await build({
  stdin: {
    contents: readFileSync(
      new URL('../components/message-info-navigation.ts', import.meta.url),
      'utf8',
    ),
    loader: 'ts',
  },
  external: ['*'],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
});
export function messageInfoNavigation(router: unknown) {
  const module = { exports: {} as { messageInfoAction: typeof messageInfoAction } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name === 'expo-router') return { router };
      if (name === '../core/conversation-scope') return scope;
      if (name === '../home/access') return access;
      throw new Error(name);
    },
  });
  return module.exports;
}
