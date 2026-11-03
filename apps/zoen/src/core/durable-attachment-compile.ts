import { build } from 'esbuild';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export const compiled = await build({
  stdin: {
    contents: readFileSync(
      process.env.ZOEN_ATTACHMENT_COMPOSER_SOURCE ??
        new URL('../media/durable-attachment-composer.tsx', import.meta.url),
      'utf8',
    ),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'native-attachment-hook-ports',
      setup(builder) {
        builder.onResolve(
          {
            filter:
              /^\.\/(?:use-durable-attachment-(?:controller)|durable-attachment-(?:types|view|pick|commands|contract|review)|attachment-gallery-journal|poll-draft-review)$/,
          },
          ({ path }) => ({
            path: fileURLToPath(
              new URL(
                `../media/${path.slice(2)}.${path.endsWith('-view') || path.endsWith('poll-draft-review') ? 'tsx' : 'ts'}`,
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
