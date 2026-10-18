import { SQLiteStorage } from 'expo-sqlite/kv-store';

// Device presentation settings only. Account data and drafts belong to the core.
const storage = new SQLiteStorage('zoen-interface.sqlite');
export const preferences = {
  get: (key: string) => storage.getItemAsync(key),
  set: (key: string, value: string) => storage.setItemAsync(key, value),
};
