export type RemoteResourceCsp = Readonly<{
  connectDomains: readonly string[];
  resourceDomains: readonly string[];
  frameDomains: readonly string[];
  baseUriDomains: readonly string[];
}>;

export type RemoteResourcePolicy = Readonly<{
  csp: RemoteResourceCsp;
  permissions: Readonly<Record<string, Record<string, never>>>;
}>;
const record = (v: unknown): v is Record<string, unknown> =>
  !!v && typeof v === 'object' && !Array.isArray(v);
function frozen<T>(value: T): T {
  if (value && typeof value === 'object') {
    for (const child of Object.values(value)) frozen(child);
    Object.freeze(value);
  }
  return value;
}

/** Standard resource CSP is a request for access, never an implicit grant. */
export function parseRemoteResourcePolicy(meta: unknown): RemoteResourcePolicy {
  const ui = record(meta) && record(meta.ui) ? meta.ui : {};
  if (ui.csp !== undefined && !record(ui.csp)) throw new Error('Invalid MCP App CSP.');
  const input = record(ui.csp) ? ui.csp : {};
  if (
    Object.keys(input).some(
      (key) =>
        !['connectDomains', 'resourceDomains', 'frameDomains', 'baseUriDomains'].includes(key),
    )
  )
    throw new Error('Unsupported MCP App CSP field.');
  const domains = (key: string): readonly string[] => {
    const values = input[key];
    if (values === undefined) return Object.freeze([]);
    if (!Array.isArray(values) || values.length > 20) throw new Error('Invalid MCP App origins.');
    return Object.freeze([
      ...new Set(
        values.map((value) => {
          if (typeof value !== 'string' || value.length > 2048)
            throw new Error('Invalid MCP App origin.');
          const url = new URL(value);
          if (
            url.protocol !== 'https:' ||
            url.username ||
            url.password ||
            url.search ||
            url.hash ||
            url.pathname !== '/' ||
            url.hostname.includes('*') ||
            !url.hostname.includes('.') ||
            /^(?:\d+\.){3}\d+$/.test(url.hostname) ||
            url.hostname.endsWith('.localhost')
          )
            throw new Error('The MCP App origin must be an exact public HTTPS origin.');
          return url.origin;
        }),
      ),
    ]);
  };
  if (ui.permissions !== undefined && !record(ui.permissions))
    throw new Error('Invalid MCP App permissions.');
  if (
    record(ui.permissions) &&
    Object.keys(ui.permissions).some(
      (name) => !['camera', 'microphone', 'geolocation', 'clipboardWrite'].includes(name),
    )
  )
    throw new Error('Unsupported MCP App permission.');
  const permissions: Record<string, Record<string, never>> = {};
  for (const name of ['camera', 'microphone', 'geolocation', 'clipboardWrite']) {
    const value = record(ui.permissions) ? ui.permissions[name] : undefined;
    if (value === undefined) continue;
    if (!record(value) || Object.keys(value).length) throw new Error('Invalid MCP App permission.');
    permissions[name] = {};
  }
  return frozen({
    csp: {
      connectDomains: domains('connectDomains'),
      resourceDomains: domains('resourceDomains'),
      frameDomains: domains('frameDomains'),
      baseUriDomains: domains('baseUriDomains'),
    },
    permissions,
  });
}
