import type { Tool } from '@modelcontextprotocol/client';
import { t } from '../i18n/index.ts';

/** Policy for this Excalidraw session, independent of the server's self-reported hints. */
export function excalidrawActionDecision(
  tool: Tool,
  args: Record<string, unknown>,
  checkpoint: string,
): 'session' | 'confirm' | 'publish' {
  const checkpointAction = tool.name === 'save_checkpoint' || tool.name === 'read_checkpoint';
  if (checkpointAction && (!checkpoint || !Object.hasOwn(args, 'id') || args.id !== checkpoint))
    throw new Error(t('connectedApps.unavailableAction'));
  if (tool.name === 'export_to_excalidraw') return 'publish';
  if (tool.annotations?.destructiveHint === true) return 'confirm';
  if (checkpointAction || tool.annotations?.readOnlyHint === true) return 'session';
  return 'confirm';
}
