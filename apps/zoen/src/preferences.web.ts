export const preferences = {
  async get(key: string): Promise<string | null> {
    return globalThis.localStorage?.getItem(`zoen.interface.${key}`) ?? null;
  },
  async set(key: string, value: string): Promise<void> {
    globalThis.localStorage.setItem(`zoen.interface.${key}`, value);
  },
};
