import { parseRemoteResourcePolicy, type RemoteResourcePolicy } from './remote-policy.ts';

export type RemoteServerId = 'excalidraw';
/** Trusted host configuration, never a URL/model-context/resource supplied by a guest.
 * This consumes a release; it does not register an App or authorize artifact access. */
export type RemoteAppBinding = Readonly<{
  id: string;
  name: string;
  endpoint: string;
  release: string;
  launchTool: string;
  resource: Readonly<{ uri: string; sha256: string; bytes: number }> & RemoteResourcePolicy;
}>;
export type RemoteAppSelection = RemoteServerId | RemoteAppBinding;
const legacy = Object.freeze({
  excalidraw: Object.freeze({ name: 'Excalidraw', endpoint: 'https://mcp.excalidraw.com/mcp' }),
});
const invalid = () => new Error('Invalid trusted remote MCP App binding.');
function object(value: unknown, keys: string[]): Record<string, unknown> {
  if (
    !value ||
    typeof value !== 'object' ||
    Array.isArray(value) ||
    Object.keys(value).sort().join(',') !== keys.sort().join(',')
  )
    throw invalid();
  return value as Record<string, unknown>;
}
function uri(value: unknown): string {
  if (typeof value !== 'string' || value.length > 2048) throw invalid();
  const url = new URL(value);
  if (url.protocol !== 'ui:' || !url.hostname || url.username || url.password) throw invalid();
  return value;
}
export function resolveRemoteApp(value: RemoteAppSelection): Readonly<{
  id: string;
  name: string;
  endpoint: string;
  binding?: RemoteAppBinding;
}> {
  if (typeof value === 'string') {
    if (value !== 'excalidraw') throw invalid();
    return Object.freeze({ id: value, ...legacy[value] });
  }
  const input = object(value, ['id', 'name', 'endpoint', 'release', 'launchTool', 'resource']);
  for (const key of ['id', 'release', 'launchTool'])
    if (typeof input[key] !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$/.test(input[key]))
      throw invalid();
  if (
    typeof input.name !== 'string' ||
    !input.name ||
    input.name.length > 120 ||
    input.name !== input.name.trim() ||
    Array.from(input.name).some(
      (character) => character.charCodeAt(0) < 32 || character.charCodeAt(0) === 127,
    ) ||
    typeof input.endpoint !== 'string' ||
    input.endpoint.length > 2048
  )
    throw invalid();
  const endpoint = new URL(input.endpoint);
  // Reuse the host's existing exact public HTTPS origin policy; no credentials/redirects.
  parseRemoteResourcePolicy({ ui: { csp: { connectDomains: [endpoint.origin] } } });
  if (
    endpoint.href !== input.endpoint ||
    endpoint.search ||
    endpoint.hash ||
    endpoint.username ||
    endpoint.password
  )
    throw invalid();
  const source = object(input.resource, ['uri', 'sha256', 'bytes', 'csp', 'permissions']);
  if (
    typeof source.sha256 !== 'string' ||
    !/^[a-f0-9]{64}$/.test(source.sha256) ||
    !Number.isSafeInteger(source.bytes) ||
    (source.bytes as number) < 1 ||
    (source.bytes as number) > 8 * 1024 * 1024
  )
    throw invalid();
  const policy = parseRemoteResourcePolicy({
    ui: { csp: source.csp, permissions: source.permissions },
  });
  const binding = Object.freeze({
    id: input.id as string,
    name: input.name,
    endpoint: endpoint.href,
    release: input.release as string,
    launchTool: input.launchTool as string,
    resource: Object.freeze({
      uri: uri(source.uri),
      sha256: source.sha256,
      bytes: source.bytes as number,
      ...policy,
    }),
  });
  return Object.freeze({ id: binding.id, name: binding.name, endpoint: binding.endpoint, binding });
}
export function sameRemotePolicy(a: RemoteResourcePolicy, b: RemoteResourcePolicy): boolean {
  return (
    Object.keys(a.csp).every((key) => {
      const name = key as keyof RemoteResourcePolicy['csp'];
      return [...a.csp[name]].sort().join('\n') === [...b.csp[name]].sort().join('\n');
    }) &&
    Object.keys(a.permissions).sort().join(',') === Object.keys(b.permissions).sort().join(',')
  );
}
