import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';

test('the maintained public wrapper forwards the current GFM receipt and retains link and quote events', async () => {
  const compiled = await build({
    entryPoints: [
      fileURLToPath(
        new URL(
          '../../node_modules/react-native-enriched-markdown/src/native/EnrichedMarkdownText.tsx',
          import.meta.url,
        ),
      ),
    ],
    bundle: true,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    write: false,
    external: ['react', 'react-native', 'react-native/*'],
  });
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const react = {
    useMemo: (read: () => unknown) => read(),
    useCallback: (fn: unknown) => fn,
    useRef: (current: unknown) => ({ current }),
    useEffect() {},
  };
  const native = {
    Platform: { OS: 'ios', select: (values: any) => values.ios ?? values.default },
    processColor: (value: unknown) => value,
    codegenNativeComponent: (name: string) => name,
  };
  const module = { exports: {} as { default: (props: any) => { type: string; props: any } } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name === 'react') return react;
      if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx };
      if (name === 'react-native') return native;
      if (name.endsWith('/codegenNativeComponent'))
        return { default: native.codegenNativeComponent };
      throw new Error(name);
    },
  });
  const source = String.raw`\*literal\* [label](https://example.test)`;
  const receipts: unknown[] = [],
    links: unknown[] = [],
    quotes: unknown[] = [];
  const view = module.exports.default({
    markdown: source,
    flavor: 'github',
    accessibilityRevision: 'row:revision',
    onResolvedAccessibilityText: (event: unknown) => receipts.push(event),
    onLinkPress: (event: unknown) => links.push(event),
    onBlockquoteToggle: (event: unknown) => quotes.push(event),
    quoteCollapse: { enabled: true },
  });
  assert.equal(view.type, 'EnrichedMarkdown');
  assert.equal(view.props.markdown, source);
  assert.equal(view.props.accessibilityRevision, 'row:revision');
  const receipt = { revision: 'row:revision', markdown: source, text: '*literal* label' };
  view.props.onResolvedAccessibilityText({ nativeEvent: receipt });
  assert.equal(receipts[0], receipt, 'the wrapper must not invent or unescape native text');
  view.props.onLinkPress({ nativeEvent: { url: 'https://example.test' } });
  assert.equal((links[0] as any).url, 'https://example.test');
  assert.equal(typeof view.props.onBlockquoteToggle, 'function');
  const quoteEvent = { nativeEvent: { source, quoteIndex: 0 } };
  view.props.onBlockquoteToggle(quoteEvent);
  assert.equal(quotes[0], quoteEvent);
});
