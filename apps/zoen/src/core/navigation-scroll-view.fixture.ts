import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { telegramPresentation } from '../theme/telegram.ts';
import { navigationGlassAppearance } from '../components/navigation-appearance.ts';

type Element = { type: any; props: Record<string, any> };
const output = await build({
  stdin: {
    contents: readFileSync(
      new URL('../components/navigation-scroll-view.tsx', import.meta.url),
      'utf8',
    ),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  external: ['*'],
});

/** Real screen composition with explicit native-scroll and React hook ports. */
export function navigationScrollFixture(platform = 'ios') {
  let tag = 0;
  const scroll = { current: null };
  const jsx = (type: unknown, props: object) => ({ type, props });
  const module: { exports: Record<string, any> } = { exports: {} };
  const ports: Record<string, unknown> = {
    react: {
      forwardRef: (render: any) => (props: any) => render(props, props.ref ?? null),
      useCallback: (callback: any) => callback,
      useRef: () => scroll,
      useState: () => [
        tag,
        (next: number) => {
          tag = next;
        },
      ],
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': {
      ScrollView: 'ScrollView',
      Text: 'Text',
      View: 'View',
      useWindowDimensions: () => ({ width: 402 }),
    },
    'expo-router': { Stack: { Screen: 'Screen' } },
    '../theme': { useTheme: () => ({ presentation: telegramPresentation('night') }) },
    './navigation-container': { NavigationContainer: 'NavigationContainer' },
    './navigation-glass': { NavigationGlass: 'NavigationGlass' },
    './navigation-appearance': { navigationGlassAppearance },
  };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    process: { env: { EXPO_OS: platform } },
    require: (name: string) => {
      if (name in ports) return ports[name];
      throw Error(`Unexpected navigation port: ${name}`);
    },
  });
  const render = (props: Record<string, any>): Element =>
    module.exports.NavigationScrollView(props);
  return { render };
}
