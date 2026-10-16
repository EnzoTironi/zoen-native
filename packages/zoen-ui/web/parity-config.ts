export const PARITY_FIXTURE = Object.freeze({ fixture_id: 'native-parity-v1', fixed_time: '2026-01-15T12:00:00Z', locale: 'en_US', timezone: 'UTC', seed: 128 });
export const PARITY_SCREENS = ['chats', 'chat', 'composer', 'spaces', 'settings', 'onboarding'] as const;
export type ParityConfig = typeof PARITY_FIXTURE & { source_revision: string; screen: typeof PARITY_SCREENS[number]; appearance: 'light' | 'dark'; viewport: 'narrow' | 'wide' };
/** Fail closed: a manifest may only attest inputs the renderer actually applies. */
export function parityConfig(value: unknown): ParityConfig {
  if (!value || typeof value !== 'object') throw new Error('Missing parity configuration.');
  const config = value as Record<string, unknown>;
  for (const [key, expected] of Object.entries(PARITY_FIXTURE)) if (config[key] !== expected) throw new Error(`Unsupported parity ${key}.`);
  if (!PARITY_SCREENS.includes(config.screen as ParityConfig['screen'])) throw new Error('Unsupported parity screen.');
  if (config.appearance !== 'light' && config.appearance !== 'dark') throw new Error('Unsupported parity appearance.');
  if (config.viewport !== 'narrow' && config.viewport !== 'wide') throw new Error('Unsupported parity viewport.');
  if (typeof config.source_revision !== 'string' || !/^[a-f0-9]{40}$/.test(config.source_revision)) throw new Error('Parity requires the full source revision.');
  return { ...PARITY_FIXTURE, source_revision: config.source_revision, screen: config.screen as ParityConfig['screen'], appearance: config.appearance, viewport: config.viewport };
}
