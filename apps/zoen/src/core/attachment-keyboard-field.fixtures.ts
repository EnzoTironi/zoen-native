import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { AttachmentKeyboardSession } from '../media/attachment-keyboard-session.ts';

const compiled = await build({
  stdin: {
    contents: readFileSync(
      new URL('../media/attachment-keyboard-context.tsx', import.meta.url),
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
      name: 'keyboard-context-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});

/** Execute the real field-binding effect with the caller's hook slots. */
export function keyboardFieldHook(
  session: AttachmentKeyboardSession,
  useLayoutEffect: (read: () => void | (() => void), deps?: readonly unknown[]) => void,
) {
  const ports: Record<string, unknown> = {
    react: { createContext: () => ({}), use: () => session, useLayoutEffect },
    'react/jsx-runtime': {},
    'react-native': {},
    'expo-crypto': {},
    'expo-router': {},
    './attachment-keyboard-session': { AttachmentKeyboardSession },
    './emoji-keyboard-panel': {},
  };
  const module = {
    exports: {} as {
      useAttachmentKeyboardField(
        tag: number | null,
        context: string,
        label: string,
        placeholder?: string,
      ): void;
    },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (path: string) => {
      assert.ok(path in ports, path);
      return ports[path];
    },
  });
  return module.exports.useAttachmentKeyboardField;
}
