import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { ConversationRestoreObservation } from './conversation-restore-observation.ts';
import type { useChatNativeMetrics } from '../components/use-chat-native-metrics.ts';

const require = createRequire(import.meta.url);
const file = new URL('../components/use-chat-native-metrics.ts', import.meta.url);
const { transformSync } = require('@babel/core');
const compiled = new Map<boolean, string>();
function compiledHook(qa: boolean) {
  const cached = compiled.get(qa);
  if (cached) return cached;
  // Expo inlines public flags at build time. Qualify both actual build variants,
  // rather than changing the VM environment after transformation.
  const previous = process.env.EXPO_PUBLIC_ZOEN_REVEAL_QA;
  process.env.EXPO_PUBLIC_ZOEN_REVEAL_QA = qa ? '1' : '0';
  try {
    const output = transformSync(readFileSync(file, 'utf8'), {
      filename: file.pathname,
      babelrc: false,
      configFile: false,
      presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
      plugins: [['babel-plugin-react-compiler', { target: '19' }]],
      caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
    }).code as string;
    assert.ok(output.includes('react/compiler-runtime'), 'the real metrics hook must be compiled');
    compiled.set(qa, output);
    return output;
  } finally {
    if (previous === undefined) delete process.env.EXPO_PUBLIC_ZOEN_REVEAL_QA;
    else process.env.EXPO_PUBLIC_ZOEN_REVEAL_QA = previous;
  }
}

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
  runInNewContext(compiledHook(!!options.qa), {
    module,
    exports: module.exports,
    process: { env: { EXPO_PUBLIC_ZOEN_REVEAL_QA: options.qa ? '1' : undefined } },
    console: { info: options.emit ?? (() => {}) },
    require(name: string) {
      if (name === 'react') return React;
      if (name === 'react/compiler-runtime' || name.startsWith('@babel/runtime/helpers/'))
        return require(name);
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
