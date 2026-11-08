import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { transformSync } from 'esbuild';
import type * as NotificationSystem from '../notifications/system';
const source = readFileSync(new URL('../notifications/system.ts', import.meta.url), 'utf8');
const { code } = transformSync(source, { loader: 'ts', format: 'cjs' });
/** Actual notification adapter; only the Expo OS boundary is supplied by the test. */
export function notificationSystemFixture(notifications: unknown) {
  const module = { exports: {} as typeof NotificationSystem };
  runInNewContext(code, {
    module,
    require: (name: string) => (name === 'expo-notifications' ? notifications : {}),
  });
  return module.exports;
}
