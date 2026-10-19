import {
  createContext,
  useContext,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
} from 'react';
import { useAudioClipPlayback, type AudioClipPlayback } from './audio-clip-playback';
import {
  originalToEditedFrame,
  voiceSelectionToEditedRange,
  type FrameRange,
  type VoiceDraft,
} from './voice-edit';
import type { VoiceRenderResult, VoiceSession } from './voice-session';

export type VoicePreviewPlaybackBinding = Readonly<{
  session: VoiceSession;
  draft: VoiceDraft | null;
  preview: VoiceRenderResult | null;
  playback: AudioClipPlayback;
  isCurrent: (observed: VoiceDraft) => boolean;
  seekOriginal: (observed: VoiceDraft, frame: number) => void;
  playFromSelection: (observed: VoiceDraft, range: FrameRange) => void;
}>;

const VoicePreviewContext = createContext<VoicePreviewPlaybackBinding | null>(null);

export function useVoicePreviewPlayback() {
  return useContext(VoicePreviewContext);
}

type Props = Readonly<{
  session: VoiceSession | null;
  ownerKey?: string;
  disabled: boolean;
  isCurrent: (session: VoiceSession) => boolean;
  children: ReactNode;
}>;

/** The player is a sibling of both presentations, so neither owns its lifetime. */
export function VoicePreviewPlayback({ session, ownerKey, disabled, isCurrent, children }: Props) {
  const [binding, setBinding] = useState<VoicePreviewPlaybackBinding | null>(null);
  return (
    <VoicePreviewContext.Provider value={binding?.session === session ? binding : null}>
      {session ? (
        <VoicePreviewPlayer
          key={ownerKey}
          session={session}
          disabled={disabled}
          isCurrent={isCurrent}
          publish={setBinding}
        />
      ) : null}
      {children}
    </VoicePreviewContext.Provider>
  );
}

function VoicePreviewPlayer({
  session,
  disabled,
  isCurrent,
  publish,
}: Omit<Props, 'session' | 'ownerKey' | 'children'> & {
  session: VoiceSession;
  publish: Dispatch<SetStateAction<VoicePreviewPlaybackBinding | null>>;
}) {
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const alive = useRef(false);
  const draft = state.draft;
  const preview =
    !disabled && state.phase === 'ready' && !state.rendering && !state.pendingTrim
      ? (session.getSendable()?.preview ?? null)
      : null;
  const current = useMemo(
    () => () =>
      alive.current &&
      !disabled &&
      isCurrent(session) &&
      !!draft &&
      !!preview &&
      session.getSnapshot().draft === draft &&
      !session.getSnapshot().rendering &&
      !session.getSnapshot().pendingTrim &&
      session.getSendable()?.preview === preview,
    [disabled, isCurrent, session, draft, preview],
  );
  const playback = useAudioClipPlayback({
    uri: preview?.uri ?? null,
    durationMs: preview?.durationMs ?? 0,
    enabled: !!preview && !disabled,
    isCurrent: current,
  });
  const binding = useMemo<VoicePreviewPlaybackBinding>(
    () => ({
      session,
      draft,
      preview,
      playback,
      isCurrent: (observed) => observed === draft && current(),
      seekOriginal(observed, frame) {
        if (observed !== draft || !current()) return;
        const projection = session.getSnapshot().projection;
        if (!projection) return;
        const edited = originalToEditedFrame(projection, frame);
        if (edited !== null)
          void playback.seekSeconds(edited / observed.original.sampleRate).catch(() => {});
      },
      playFromSelection(observed, range) {
        if (observed !== draft || !current()) return;
        const projection = session.getSnapshot().projection;
        if (!projection) return;
        const edited = voiceSelectionToEditedRange(projection, range);
        if (edited)
          void playback.playFrom(edited.startFrame / observed.original.sampleRate).catch(() => {});
      },
    }),
    [session, draft, preview, playback, current],
  );
  useLayoutEffect(() => {
    alive.current = true;
    // Session publishes synchronously before retiring a native source/render lease.
    // Revocation cannot wait for React's next render or the player's hook cleanup.
    const unsubscribe = session.subscribe(() => {
      if (!current()) playback.invalidate();
    });
    if (!current()) playback.invalidate();
    return () => {
      alive.current = false;
      unsubscribe();
      playback.invalidate();
    };
  }, [session, playback, current]);
  useLayoutEffect(() => {
    publish(binding);
    return () => publish((value) => (value === binding ? null : value));
  }, [publish, binding]);
  return null;
}
