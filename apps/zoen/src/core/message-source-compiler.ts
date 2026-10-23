import type { PluginBuild } from 'esbuild';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};

export const messageSourceModules = [
  'conversation-message.tsx',
  'use-conversation-message.ts',
  'conversation-message-face.tsx',
  'conversation-message-details.tsx',
  'message-interaction.tsx',
];

/** Compile extracted production modules using the same preset as their public entry. */
export function compileMessageSource(path: string, platform: 'ios' | 'web', source?: string) {
  return transformSync(source ?? readFileSync(path, 'utf8'), {
    filename: path,
    configFile: false,
    babelrc: false,
    presets: [require.resolve('babel-preset-expo')],
    caller: {
      name: 'metro',
      platform,
      engine: 'hermes',
      isDev: false,
      supportsReactCompiler: true,
    },
  }).code;
}

export function transformMessageModules(builder: PluginBuild, platform: 'ios' | 'web') {
  builder.onLoad(
    { filter: /(?:conversation-message(?:-face|-details)?\.tsx|use-conversation-message\.ts)$/ },
    ({ path }) => ({
      contents: compileMessageSource(path, platform),
      loader: 'js',
    }),
  );
}
