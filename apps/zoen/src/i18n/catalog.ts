export type MessageTree = { readonly [key: string]: string | MessageTree };
export type CatalogShape<T> = {
  readonly [K in keyof T]: T[K] extends string ? string : CatalogShape<T[K]>;
};
export type FeatureCatalog<T extends MessageTree = MessageTree> = {
  readonly en: T;
  readonly 'pt-BR': CatalogShape<T>;
};

/** Each UI owner writes one feature file; the integration owner only assembles imports. */
export function defineFeature<const T extends MessageTree>(
  en: T,
  ptBR: CatalogShape<T>,
): FeatureCatalog<T> {
  return { en, 'pt-BR': ptBR };
}
export type Plural = { readonly one: string; readonly other: string };
export type LeafPaths<T> = {
  [K in keyof T & string]: T[K] extends string | Plural ? K : `${K}.${LeafPaths<T[K]>}`;
}[keyof T & string];
export type At<T, P extends string> = P extends `${infer K}.${infer Tail}`
  ? K extends keyof T
    ? At<T[K], Tail>
    : never
  : P extends keyof T
    ? T[P]
    : never;
type Text<T> = T extends string
  ? T
  : T extends Readonly<Record<string, string>>
    ? T[keyof T]
    : never;
type Tokens<S extends string> = S extends `${string}%{${infer Key}}${infer Tail}`
  ? Key | Tokens<Tail>
  : never;
export type Params<T> = {
  [K in Tokens<Text<T>>]: K extends 'count' ? number : string | number | bigint;
} & (T extends Plural ? { count: number } : unknown);
export type Args<T> = keyof Params<T> extends never ? [options?: undefined] : [options: Params<T>];

function strings(tree: MessageTree, prefix = ''): Map<string, string> {
  const result = new Map<string, string>();
  for (const [key, value] of Object.entries(tree)) {
    if (!key || key.includes('.'))
      throw new Error('Catalog keys must be nonempty and cannot contain dots.');
    const path = prefix ? `${prefix}.${key}` : key;
    if (typeof value === 'string') result.set(path, value);
    else for (const [child, text] of strings(value, path)) result.set(child, text);
  }
  return result;
}
const tokens = (text: string) =>
  [...new Set([...text.matchAll(/%\{([^}]+)\}/g)].map((match) => match[1]))].sort().join(',');
/** CI/startup invariant, never a runtime text-to-text translation shim. */
export function assertCatalogParity(catalog: FeatureCatalog): void {
  const en = strings(catalog.en),
    pt = strings(catalog['pt-BR']);
  if (en.size !== pt.size || [...en.keys()].some((key) => !pt.has(key)))
    throw new Error('Catalog language keys differ.');
  for (const [key, text] of en)
    if (tokens(text) !== tokens(pt.get(key)!))
      throw new Error(`Catalog interpolation differs: ${key}`);
}
