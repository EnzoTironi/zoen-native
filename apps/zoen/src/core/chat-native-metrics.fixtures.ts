import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as React from 'react';
import { ConversationRestoreObservation } from './conversation-restore-observation.ts';
import type { useChatNativeMetrics } from '../components/use-chat-native-metrics.ts';

const output = await build({
  stdin: {
    contents: readFileSync(
      new URL('../components/use-chat-native-metrics.ts', import.meta.url),
      'utf8',
    ),
    loader: 'ts',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'real-chat-metrics-hook',
      setup(b) {
        b.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});

/** Actual compiled hook; platform lifecycle/native samples are explicit ports. */
export function chatNativeMetricsModule(options: {
  qa?: boolean;
  native?: boolean;
  appState?: {
    currentState: string;
    addEventListener(event: string, callback: () => void): { remove(): void };
  };
  useFocusEffect(callback: () => void | (() => void)): void;
  emit?: (line: string) => void;
}) {
  const module = { exports: {} as { useChatNativeMetrics: typeof useChatNativeMetrics } };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    process: { env: { EXPO_PUBLIC_ZOEN_REVEAL_QA: options.qa ? '1' : undefined } },
    console: { info: options.emit ?? (() => {}) },
    require(name: string) {
      if (name === 'react') return React;
      if (name === 'expo-router') return { useFocusEffect: options.useFocusEffect };
      if (name === 'react-native') return { AppState: options.appState };
      if (name === '../core/conversation-restore-observation')
        return { ConversationRestoreObservation };
      if (name === './navigation-container')
        return { navigationMetricsAvailable: !!options.native };
      throw Error(`Unexpected metrics-hook import: ${name}`);
    },
  });
  return module.exports;
}
