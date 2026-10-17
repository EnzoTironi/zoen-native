import { useRef, useState, type RefObject } from 'react';
import { useRecorder } from './media.tsx';
import type { Conversation, Messages } from './messages.ts';
import type { NativeNavigationVoice } from './native-navigation.tsx';

/** Root voice uses the existing direct Zoen conversation; never creates a synthetic chat. */
export function rootVoiceDestination(conversations: readonly Conversation[]) {
  return conversations.find(c => c.kind === 'direct' && c.counterpart?.handle === 'zoen' && c.can_send)?.id ?? null;
}
export function useRootVoice({ conversations, messages, select, onNotice }: {
  conversations: readonly Conversation[];
  messages: RefObject<Messages | null>;
  select: (id: string) => void;
  onNotice: (text: string) => void;
}): NativeNavigationVoice {
  const destination = useRef<string | null>(null);
  const taking = useRef(false);
  const finishing = useRef(false);
  const [active, setActive] = useState(false);
  const [locked, setLocked] = useState(false);
  const recorder = useRecorder(clip => {
    finishing.current = false;
    const id = destination.current;
    const controller = messages.current;
    if (!id || !controller) { onNotice('Zoen voice is unavailable without a connected conversation.'); return; }
    if (clip.seconds < 0.6) { onNotice('Recording is too short to send.'); return; }
    void controller.sendFile(id, {
      name: `Mensagem de voz.${clip.mime.includes('mp4') ? 'm4a' : 'webm'}`,
      mime: clip.mime, bytes: clip.bytes,
      voice: { ms: Math.round(clip.seconds * 1000), levels: clip.levels, transcript: '', reply: null },
    }).then(() => select(id)).catch((error: unknown) => onNotice(error instanceof Error ? error.message : 'Could not send voice recording.'));
  }, why => { finishing.current = false; taking.current = false; setActive(false); setLocked(false); onNotice(why); });
  return {
    active, locked, seconds: recorder.seconds, levels: recorder.recording?.levels ?? [],
    start: () => {
      if (taking.current || finishing.current) return;
      const id = rootVoiceDestination(conversations);
      if (!messages.current || !id) { onNotice('Zoen voice requires an existing connected direct conversation with Zoen.'); return; }
      taking.current = true; destination.current = id; setActive(true); setLocked(false); void recorder.start();
    },
    lock: () => setLocked(true),
    send: () => { if (!taking.current) return; taking.current = false; finishing.current = recorder.recording !== null; setActive(false); setLocked(false); recorder.send(); },
    cancel: () => { if (!taking.current) return; taking.current = false; setActive(false); setLocked(false); recorder.cancel(); },
  };
}
