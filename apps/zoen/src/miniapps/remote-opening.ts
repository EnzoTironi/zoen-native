import { sha256 } from '@noble/hashes/sha2.js';
import { t } from '../i18n/index.ts';
import {
  resolveRemoteApp,
  type RemoteAppBinding,
  type RemoteAppSelection,
} from './remote-binding.ts';
import { excalidrawPresentation } from './excalidraw-presentation.ts';
import { remoteAppHtml } from './remote-sandbox.ts';
import {
  captureRemoteArguments,
  type RemoteMcpSession,
  type RemoteAppResource,
  type RemoteMcpDescriptor,
} from './remote-session.ts';
import type { CallToolResult } from '@modelcontextprotocol/client';

/** Supplied by trusted host code, never reconstructed from route parameters or a guest. */
export type RemoteAppLaunch = Readonly<{
  binding: RemoteAppBinding;
  arguments: Record<string, unknown>;
}>;
export type RemoteAppScreenProps =
  | { id: string; launch?: never }
  | { launch: RemoteAppLaunch; id?: never };
export type RemoteOpening = Readonly<{
  key: string;
  title: string;
  selection: RemoteAppSelection;
  arguments: Readonly<Record<string, unknown>>;
  legacyDrawing: boolean;
}>;
export type OpenedRemoteApp = {
  descriptor: RemoteMcpDescriptor;
  resource: RemoteAppResource;
  result: CallToolResult;
  arguments: Record<string, unknown>;
  html: string;
  checkpoint?: string;
};

export function captureRemoteOpening(props: RemoteAppScreenProps): RemoteOpening | null {
  let selection: RemoteAppSelection;
  let args: Record<string, unknown>;
  if (props.launch) {
    if (props.id !== undefined) throw new Error('Ambiguous remote MCP App selection.');
    const resolved = resolveRemoteApp(props.launch.binding);
    if (!resolved.binding) throw new Error('An external App requires a pinned release.');
    selection = resolved.binding;
    args = props.launch.arguments;
  } else {
    if (props.id !== 'excalidraw') return null;
    selection = 'excalidraw';
    args = {
      elements: JSON.stringify([
        {
          id: 'idea',
          type: 'rectangle',
          x: 0,
          y: 0,
          width: 240,
          height: 120,
          backgroundColor: '#e3f3e8',
          strokeColor: '#1e1e1e',
          label: { text: t('connectedApps.idea') },
        },
      ]),
    };
  }
  const captured = captureRemoteArguments(args);
  const key = Array.from(
    sha256(new TextEncoder().encode(JSON.stringify([selection, captured]))),
    (byte) => byte.toString(16).padStart(2, '0'),
  ).join('');
  return Object.freeze({
    key,
    selection,
    arguments: captured,
    title: resolveRemoteApp(selection).name,
    legacyDrawing: selection === 'excalidraw',
  });
}

/** Shared opening path for the real screen: SDK admission precedes launch and frame bytes. */
export async function openRemoteApp(
  connection: RemoteMcpSession,
  opening: RemoteOpening,
  check: () => void,
): Promise<OpenedRemoteApp> {
  check();
  const descriptor = await connection.connect();
  check();
  const upstream = await connection.readApp();
  check();
  const resource = opening.legacyDrawing ? excalidrawPresentation(descriptor, upstream) : upstream;
  if (opening.legacyDrawing && descriptor.mainToolName !== 'create_view')
    throw new Error(t('connectedApps.unavailable'));
  const result = await connection.call(descriptor.mainToolName, opening.arguments, {
    origin: 'host',
  });
  check();
  if (result.isError) throw new Error(t('connectedApps.appUnavailable'));
  let checkpoint: string | undefined;
  if (opening.legacyDrawing) {
    const structured = result.structuredContent;
    const candidate =
      structured && typeof structured === 'object' && 'checkpointId' in structured
        ? structured.checkpointId
        : undefined;
    if (typeof candidate !== 'string' || !/^[A-Za-z0-9_-]{1,128}$/.test(candidate))
      throw new Error(t('connectedApps.unavailable'));
    checkpoint = candidate;
  }
  return {
    descriptor,
    resource,
    result,
    arguments: opening.arguments,
    html: remoteAppHtml(resource.html, resource.csp),
    checkpoint,
  };
}
