export type ChatPreferences = { pinned?: boolean; muted?: boolean; archived?: boolean };
type PreferenceStorage = Pick<Storage, 'getItem' | 'setItem'>;

// Legacy preferences have no account owner. Do not migrate them into the next login.
const key = (identity: string) => `zoen.chat-prefs:${identity}`;

export function loadChatPreferences(storage: PreferenceStorage, identity: string): Record<string, ChatPreferences> {
  try {
    const value: unknown = JSON.parse(storage.getItem(key(identity)) ?? '{}');
    if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
    return Object.fromEntries(Object.entries(value).flatMap(([id, item]) => {
      if (!item || typeof item !== 'object' || Array.isArray(item)) return [];
      const preference = item as Record<string, unknown>;
      return [[id, Object.fromEntries(['pinned', 'muted', 'archived']
        .filter((name) => typeof preference[name] === 'boolean')
        .map((name) => [name, preference[name]]))]];
    }));
  } catch { return {}; }
}

export function saveChatPreferences(storage: PreferenceStorage, identity: string, preferences: Record<string, ChatPreferences>): void {
  try { storage.setItem(key(identity), JSON.stringify(preferences)); }
  catch { /* Keep the current in-memory choice when browser storage is unavailable. */ }
}
