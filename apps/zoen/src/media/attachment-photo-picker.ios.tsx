import { randomUUID } from 'expo-crypto';
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { AppState } from 'react-native';
import { AttachmentPhotoSession, type PhotoPickerSubmission } from './attachment-photo-session';
import { photoLibrary } from './attachment-photo-library';
import { pickAttachmentAssets } from './attachment-picker';
import { useAttachmentKeyboardSession } from './attachment-keyboard-context';
import type { KeyboardPresentation } from './attachment-keyboard-session';
import { AttachmentPhotoKeyboard } from './attachment-photo-keyboard.ios';

export function useAttachmentPhotoPicker() {
  const keyboard = useAttachmentKeyboardSession();
  const [keyboardRequest, setKeyboardRequest] = useState<KeyboardPresentation | null>(null);
  const pickerOwner = useRef<string | null>(null);
  const [mountedOwner, setMountedOwner] = useState(false);
  const preferredIndex = useRef<number | null>(null);
  const preferred = useRef<string | null>(null);
  const submission = useRef<PhotoPickerSubmission | null>(null);
  const session = useMemo(
    () =>
      new AttachmentPhotoSession({
        ...photoLibrary,
        capture: (remaining, current) => pickAttachmentAssets('camera', remaining, current),
        browse: (remaining, current) => pickAttachmentAssets('document', remaining, current),
      }),
    [],
  );
  const state = useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
  useEffect(() => {
    const unsubscribe = keyboard?.subscribe(() => {
      if (pickerOwner.current && !keyboard.ownsPicker(pickerOwner.current)) session.cancel();
    });
    return () => {
      unsubscribe?.();
    };
  }, [keyboard, session]);
  useEffect(() => {
    // Registering the native observer performs a PHAsset fetch: never do it at chat mount.
    if (state.visible && state.permission?.granted)
      return photoLibrary.subscribe(session.invalidate);
  }, [session, state.visible, state.permission?.granted]);
  useEffect(() => {
    const background = AppState.addEventListener('change', (value) => {
      if (value === 'background') session.cancel();
    });
    return () => {
      background.remove();
      session.cancel();
    };
  }, [session]);
  return {
    pick: async (remaining: number, current: () => boolean) => {
      preferredIndex.current = null;
      preferred.current = null;
      submission.current = null;
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
    cancel: session.cancel,
    view: keyboardRequest ? (
      <AttachmentPhotoKeyboard
        state={state}
        session={session}
        keyboard={mountedOwner ? keyboard : null}
        request={keyboardRequest}
        onResume={setKeyboardRequest}
        onPreferredIndex={(index) => {
          preferredIndex.current = index;
        }}
        onSubmission={(intent) => {
          submission.current = intent;
        }}
      />
    ) : null,
  };
}
