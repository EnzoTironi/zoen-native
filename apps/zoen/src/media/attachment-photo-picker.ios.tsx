import { randomUUID } from 'expo-crypto';
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import {
  AttachmentPhotoSession,
  type PhotoPickerAdmission,
  type PhotoPickerSubmission,
} from './attachment-photo-session';
import { photoLibrary } from './attachment-photo-library';
import { pickAttachmentAssets } from './attachment-picker';
import { useAttachmentKeyboardSession } from './attachment-keyboard-context';
import type { KeyboardPresentation } from './attachment-keyboard-session';
import { AttachmentPhotoKeyboard } from './attachment-photo-keyboard.ios';
import {
  AttachmentRecentSession,
  type RecentAttachmentConfiguration,
} from './attachment-recent-session';
import CoreModule from '../../modules/zoen-core/src/ZoenCoreModule';

export function useAttachmentPhotoPicker(recentConfiguration?: RecentAttachmentConfiguration) {
  const keyboard = useAttachmentKeyboardSession();
  const [keyboardRequest, setKeyboardRequest] = useState<KeyboardPresentation | null>(null);
  const pickerOwner = useRef<string | null>(null);
  const [mountedOwner, setMountedOwner] = useState(false);
  const preferredIndex = useRef<number | null>(null);
  const preferred = useRef<string | null>(null);
  const submission = useRef<PhotoPickerSubmission | null>(null);
  const [sendAsFileAllowed, setSendAsFileAllowed] = useState(false);
  const session = useMemo(
    () =>
      new AttachmentPhotoSession({
        ...photoLibrary,
        discard: (uri) => {
          // A recent source is a native-owned verified cache lease. Releasing
          // it also removes its source binding; SDK cache copies use the same
          // existing cache remover. Native failure retains tracked cleanup.
          void CoreModule?.releaseLocalFile(uri).catch(() => {});
          photoLibrary.discard(uri);
        },
        capture: (remaining, current) => pickAttachmentAssets('camera', remaining, current),
        browse: (remaining, current, category) =>
          pickAttachmentAssets(
            'document',
            remaining,
            current,
            category === 'audio' ? { mime: 'audio/*' } : undefined,
          ),
      }),
    [],
  );
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  const recent = useMemo(() => new AttachmentRecentSession(), []);
  const recentState = useSyncExternalStore(
    recent.subscribe,
    recent.getSnapshot,
    recent.getSnapshot,
  );
  const cancel = useMemo(
    () => () => {
      recent.retire();
      session.cancel();
    },
    [recent, session],
  );
  const photoOwnerCurrent = session.isCurrent(state.token);
  useEffect(() => {
    if (!recentConfiguration || !photoOwnerCurrent || state.category === 'photos') {
      recent.retire();
      return;
    }
    recent.activate(
      recentConfiguration,
      state.category === 'audio' ? 'audio' : 'file',
      session.remaining(),
      () => session.isCurrent(state.token),
    );
  }, [recent, recentConfiguration, session, state.category, state.token, photoOwnerCurrent]);
  useEffect(() => () => recent.retire(), [recent]);
  useEffect(() => {
    const unsubscribe = keyboard?.subscribe(() => {
      if (pickerOwner.current && !keyboard.ownsPicker(pickerOwner.current)) cancel();
    });
    return () => {
      unsubscribe?.();
    };
  }, [keyboard, cancel]);
  useEffect(() => {
    // Registering the native observer performs a PHAsset fetch: never do it at chat mount.
    if (state.visible && state.permission?.granted)
      return photoLibrary.subscribe(session.invalidate);
  }, [session, state.visible, state.permission?.granted]);
  useEffect(() => {
    const background = AppState.addEventListener('change', (value) => {
      if (value === 'background') cancel();
    });
    return () => {
      background.remove();
      cancel();
    };
  }, [cancel]);
  return {
    pick: async (remaining: number, current: () => boolean, admission?: PhotoPickerAdmission) => {
      preferredIndex.current = null;
      preferred.current = null;
      submission.current = null;
      setSendAsFileAllowed(admission?.sendAsFileAllowed === true);
      const presentation = keyboard?.getSnapshot().presentation;
      const request =
        presentation?.kind === 'attachments' && keyboard?.admitPicker(presentation.id)
          ? presentation
          : null;
      if (presentation?.kind === 'attachments' && !request) return [];
      pickerOwner.current = request?.id ?? null;
      setMountedOwner(request !== null);
      setKeyboardRequest(
        request ?? {
          id: randomUUID(),
          kind: 'attachments',
          field: { tag: 0, context: `photos:${randomUUID()}`, label: '', placeholder: '' },
          closing: false,
          returnToKeyboard: false,
        },
      );
      const files = await session.open(remaining, current);
      if (pickerOwner.current === (request?.id ?? null)) pickerOwner.current = null;
      setKeyboardRequest(null);
      preferred.current =
        preferredIndex.current === null ? null : (files[preferredIndex.current]?.uri ?? null);
      return files;
    },
    preferredUri: () => preferred.current,
    submission: () => submission.current,
    cancel,
    view: keyboardRequest ? (
      <AttachmentPhotoKeyboard
        state={state}
        session={session}
        keyboard={mountedOwner ? keyboard : null}
        request={keyboardRequest}
        sendAsFileAllowed={sendAsFileAllowed}
        onResume={setKeyboardRequest}
        onPreferredIndex={(index) => {
          preferredIndex.current = index;
        }}
        onSubmission={(intent) => {
          submission.current = intent;
        }}
        recent={recentConfiguration ? { session: recent, state: recentState } : undefined}
      />
    ) : null,
  };
}
