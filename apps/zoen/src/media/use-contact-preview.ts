import { useSyncExternalStore } from 'react';
import { useFilePreview } from './use-file-preview';
import { ContactPreviewSession } from './contact-preview-session';
import type { MediaDriver } from './types';

export function useContactPreview(props: {
  media: MediaDriver;
  source: { itemId: string; version?: number; expectedHash: string };
  spaceId?: string;
  isCurrent(): boolean;
}) {
  const result = useFilePreview({
    ...props,
    create: (current) => new ContactPreviewSession({ ...props, current }),
  });
  const { session } = result;
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  return { ...result, state };
}
