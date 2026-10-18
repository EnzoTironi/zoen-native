import html, { manifest } from './vendor/excalidraw-ui.ts';
import type { RemoteAppResource, RemoteMcpDescriptor, RemoteResourceCsp } from './remote-session';

const upstreamUri = 'ui://excalidraw/mcp-app.html';
const channels = ['connectDomains', 'resourceDomains', 'frameDomains', 'baseUriDomains'] as const;

/** A pinned, explicitly patched renderer; the remote MCP connection remains authoritative. */
export function excalidrawPresentation(
  descriptor: RemoteMcpDescriptor,
  upstream: RemoteAppResource,
): RemoteAppResource {
  if (
    descriptor.server.id !== 'excalidraw' ||
    descriptor.server.endpoint !== 'https://mcp.excalidraw.com/mcp' ||
    descriptor.mainToolName !== 'create_view' ||
    descriptor.resourceUri !== upstreamUri ||
    upstream.resourceUri !== upstreamUri ||
    upstream.mimeType !== manifest.mimeType
  )
    throw new Error('Unsupported Excalidraw presentation.');
  const csp: RemoteResourceCsp = manifest.csp;
  for (const channel of channels) {
    if (csp[channel].some((origin) => !upstream.csp[channel].includes(origin)))
      throw new Error('The Excalidraw presentation exceeds the server resource policy.');
  }
  return Object.freeze({
    html,
    mimeType: 'text/html;profile=mcp-app',
    resourceUri: manifest.resourceUri,
    sha256: manifest.sha256,
    bytes: manifest.bytes,
    csp,
    permissions: Object.freeze({}),
  });
}
