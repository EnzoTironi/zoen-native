import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { createLocalization } from '../i18n/runtime.ts';
import { chat } from '../i18n/catalogs/chat.ts';
import { common } from '../i18n/catalogs/common.ts';
export const source = fileURLToPath(new URL('../components/message-markdown.tsx', import.meta.url));
const nativePorts = new Set([
  'react',
  'react/jsx-runtime',
  'react-native',
  'react-native-enriched-markdown',
  '../theme',
  './message-context-ports',
  './conversation-hashtag-search',
  './use-quote-collapse',
  './quote-layout-context',
  'expo-clipboard',
  'expo-haptics',
  '../i18n/index.ts',
]);
const compiled = await build({
  entryPoints: [source],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'message-markdown-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          nativePorts.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});

type AlertRecord = {
  title: string;
  message: string;
  buttons: { text: string; style?: string; onPress: () => void }[];
  options: { cancelable: boolean; onDismiss: () => void };
};
export function fixture(
  platform = 'ios',
  options: {
    launch?: () => Promise<unknown>;
    copy?: () => Promise<boolean>;
    language?: 'en' | 'pt-BR';
    hashtag?: (value: string) => void;
  } = {},
) {
  const opened: string[] = [];
  const copied: string[] = [];
  const alerts: AlertRecord[] = [];
  const localization = createLocalization({ chat, common }, [options.language ?? 'en']);
  const colors = {
    accent: '#1478d4',
    secondary: '#798088',
    surface: '#232933',
    muted: '#30353f',
    text: '#ffffff',
    line: '#3b424b',
  };
  type Slot = { deps?: unknown[]; value?: unknown; cleanup?: () => void };
  const slots: Slot[] = [];
  let cursor = 0;
  const effects: (() => void)[] = [];
  const slot = () => slots[cursor++] ?? (slots[cursor - 1] = {});
  const same = (a: unknown[] | undefined, b: unknown[]) =>
    a && a.length === b.length && a.every((value, i) => Object.is(value, b[i]));
  const hashtagContext = {};
  const module = { exports: {} as Record<string, unknown> };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: platform } },
    URL,
    TextEncoder,
    require(name: string) {
      if (name === 'react')
        return {
          use: (context: unknown) =>
            context === hashtagContext ? (options.hashtag ?? null) : null,
          memo: (value: unknown) => value,
          useMemo: (read: () => unknown, deps: unknown[]) => {
            const owner = slot();
            if (!same(owner.deps, deps)) {
              owner.deps = deps;
              owner.value = read();
            }
            return owner.value;
          },
          useRef: (initial: unknown) => {
            const owner = slot();
            if (!('value' in owner)) owner.value = { current: initial };
            return owner.value;
          },
          useLayoutEffect: (read: () => (() => void) | undefined, deps: unknown[]) => {
            const owner = slot();
            if (!same(owner.deps, deps)) {
              owner.deps = deps;
              effects.push(() => {
                owner.cleanup?.();
                owner.cleanup = read();
              });
            }
          },
        };
      if (name === 'react/jsx-runtime')
        return { jsx: (type: unknown, props: unknown) => ({ type, props }) };
      if (name === 'react-native')
        return {
          Text: 'PlainText',
          Alert: {
            alert: (...args: [string, string, AlertRecord['buttons'], AlertRecord['options']]) => {
              const [title, message, buttons, options] = args;
              alerts.push({ title, message, buttons, options });
            },
          },
          Linking: {
            openURL: (url: string) => {
              opened.push(url);
              return options.launch?.() ?? Promise.resolve();
            },
          },
        };
      if (name === 'expo-haptics') return { selectionAsync: async () => {} };
      if (name === 'expo-clipboard')
        return {
          setStringAsync: (text: string) => {
            copied.push(text);
            return options.copy?.() ?? Promise.resolve(true);
          },
        };
      if (name === '../i18n/index.ts') return localization;
      if (name === 'react-native-enriched-markdown')
        return { EnrichedMarkdownText: 'NativeMarkdownText' };
      if (name === './use-quote-collapse') return { useQuoteCollapse: () => ({}) };
      if (name === './quote-layout-context') return { useQuoteLayoutTransition: () => null };
      if (name === './conversation-hashtag-search')
        return { ConversationHashtagSearch: hashtagContext };
      if (name === './message-context-ports') return { MessageTextSelection: {} };
      if (name === '../theme') return { useTheme: () => ({ colors }) };
      throw new Error(`Unexpected native port ${name}`);
    },
  });
  const render = module.exports.MessageMarkdown as (props: Record<string, unknown>) => {
    type: unknown;
    props: Record<string, unknown>;
  };
  return {
    opened,
    copied,
    alerts,
    colors,
    render: (props: Record<string, unknown>) => {
      cursor = 0;
      const tree = render(props);
      for (const effect of effects.splice(0)) effect();
      return tree;
    },
    close: () => {
      for (const owner of slots) owner.cleanup?.();
    },
  };
}
