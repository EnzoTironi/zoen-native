import { build } from 'esbuild';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
export const files = [
  'MediaComposer',
  'voice-review.ios',
  'voice-waveform.ios',
  'voice-dock',
] as const;
export const compiled = Object.fromEntries(
  await Promise.all(
    files.map(async (name) => {
      const output = await build({
        stdin: {
          contents: readFileSync(new URL(`../media/${name}.tsx`, import.meta.url), 'utf8'),
          loader: 'tsx',
        },
        bundle: true,
        platform: 'node',
        format: 'cjs',
        jsx: 'automatic',
        write: false,
        plugins: [
          {
            name: 'native-voice-ports',
            setup(builder) {
              builder.onResolve(
                { filter: /^\.\/(?:native-capture-owner|call-audio-focus|audio-mode)\.ts$/ },
                ({ path }) => ({
                  path: fileURLToPath(new URL(`../media/${path.slice(2)}`, import.meta.url)),
                }),
              );
              builder.onResolve(
                {
                  filter:
                    /^\.\/(?:voice-waveform-(?:model|precision)|(?:use-)?media-composer-(?:controller|capture|shared|selection|view|lifecycle|contract))$/,
                },
                ({ path }) => ({
                  path: fileURLToPath(
                    new URL(
                      `../media/${path.slice(2)}.${path.endsWith('precision') || path.endsWith('view') ? 'tsx' : 'ts'}`,
                      import.meta.url,
                    ),
                  ),
                }),
              );
              builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
            },
          },
        ],
      });
      return [name, output.outputFiles[0]!.text];
    }),
  ),
);
