import { useCallback, useEffect, useRef, useState } from 'react';
import { AppState, Modal, Platform, ScrollView, Text, View } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { Image } from 'expo-image';
import * as DocumentPicker from 'expo-document-picker';
import * as ImagePicker from 'expo-image-picker';
import {
  AudioModule,
  RecordingPresets,
  setAudioModeAsync,
  useAudioRecorder,
  useAudioRecorderState,
} from 'expo-audio';
import { Host, BottomSheet, Column, Button } from '@expo/ui';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useTheme } from '../theme';
import { IconButton, NativeButton } from '../components/primitives';
import { useDialogs } from '../components/dialogs';
import { useClient } from '../core/provider';
import { AudioClip, clipTime, pauseAudioPlayback } from './AudioClip';
import type { FileSelection } from './types';
import { discardCacheCopy } from './cache-files';
import { sensitiveCacheWork } from './cache-work';
import { MAX_SELECTED_BYTES } from './types';
import { VideoClip } from './VideoClip';

type Recording = { uri: string; durationMs: number; levels: number[] };
const failureMessage = (reason: unknown) =>
  reason instanceof Error ? reason.message : 'Não foi possível concluir. Tente novamente.';
async function discard(uri: string | undefined) {
  if (Platform.OS === 'web') {
    if (uri?.startsWith('blob:')) URL.revokeObjectURL(uri);
    return;
  }
  try {
    discardCacheCopy(uri);
  } catch {
    /* Cache eviction also disposes a staged selection. */
  }
}

/** Explicit record → review → send. An interrupted recording is never posted. */
export function MediaComposer({
  spaceId,
  replyTo = null,
  thread = false,
  disabled = false,
  onSent,
}: {
  spaceId: string;
  replyTo?: string | null;
  thread?: boolean;
  disabled?: boolean;
  onSent?: () => void;
}) {
  const { store } = useClient();
  const media = store.driver.media;
  const { colors, scheme } = useTheme();
  const insets = useSafeAreaInsets();
  const { inform } = useDialogs();
  const recorder = useAudioRecorder({
    ...RecordingPresets.HIGH_QUALITY,
    numberOfChannels: 1,
    bitRate: 64000,
    isMeteringEnabled: true,
  });
  const recorderState = useAudioRecorderState(recorder, 100);
  const [menu, setMenu] = useState(false);
  const [selection, setSelection] = useState<FileSelection | null>(null);
  const [clip, setClip] = useState<Recording | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const levels = useRef<number[]>([]);
  const stopTask = useRef<Promise<void> | null>(null);
  const recording = useRef(false);
  const uriToDiscard = useRef<string | undefined>(undefined);
  const mounted = useRef(true);
  const focused = useRef(false);
  const selecting = useRef(false);
  useEffect(() => {
    uriToDiscard.current = selection?.uri ?? clip?.uri;
  }, [selection, clip]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      void discard(uriToDiscard.current);
    };
  }, []);
  useEffect(() => {
    if (!recorderState.isRecording) return;
    if (typeof recorderState.metering === 'number')
      levels.current.push(Math.min(1, Math.max(0.02, 10 ** (recorderState.metering / 40))));
  }, [recorderState.isRecording, recorderState.durationMillis, recorderState.metering]);

  const stop = useCallback(() => {
    if (stopTask.current) return stopTask.current;
    if (!recording.current) return Promise.resolve();
    recording.current = false;
    const task = (async () => {
      try {
        const durationMs = recorder.getStatus().durationMillis;
        await recorder.stop();
        const uri = recorder.uri;
        if (uri && durationMs >= 250 && mounted.current && focused.current) {
          const stride = Math.max(1, Math.ceil(levels.current.length / 160));
          setClip({
            uri,
            durationMs: Math.round(durationMs),
            levels: levels.current.filter((_, index) => index % stride === 0),
          });
        } else {
          await discard(uri ?? undefined);
          if (mounted.current && focused.current && durationMs < 250)
            void inform('Gravação muito curta', 'Grave por pelo menos um segundo para enviar.');
        }
      } catch (reason) {
        if (mounted.current) setFailure(failureMessage(reason));
      } finally {
        await setAudioModeAsync({ allowsRecording: false });
        stopTask.current = null;
      }
    })();
    stopTask.current = task;
    return sensitiveCacheWork.track(task);
  }, [recorder, inform]);
  useEffect(() => {
    if (Platform.OS !== 'ios') return;
    return store.registerDeviceCleanup(async () => {
      const uri = mounted.current ? recorder.uri : undefined;
      await stop();
      // SDK58 releases this SharedObject on unmount and synchronously stops
      // its native recorder. A retained retry must never call that released object.
      if (mounted.current && recorder.getStatus().isRecording) await recorder.stop();
      if (mounted.current && recorder.getStatus().isRecording)
        throw new Error('Não foi possível encerrar a gravação deste aparelho.');
      discardCacheCopy(uriToDiscard.current);
      discardCacheCopy(uri ?? undefined);
    });
  }, [store, stop, recorder]);
  useFocusEffect(
    useCallback(() => {
      focused.current = true;
      return () => {
        focused.current = false;
        void stop();
      };
    }, [stop]),
  );
  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => {
      if (state !== 'active') void stop();
    });
    return () => subscription.remove();
  }, [stop]);
  useEffect(() => {
    if (recorderState.isRecording && recorderState.durationMillis >= 3_600_000) void stop();
  }, [recorderState.isRecording, recorderState.durationMillis, stop]);

  async function pick(kind: 'document' | 'photo' | 'camera') {
    try {
      await sensitiveCacheWork.run(() => pickSelection(kind));
    } catch (reason) {
      if (mounted.current) setFailure(failureMessage(reason));
    }
  }
  async function pickSelection(kind: 'document' | 'photo' | 'camera') {
    if (!media || busy || disabled || selecting.current || !focused.current) return;
    selecting.current = true;
    setBusy(true);
    setMenu(false);
    setFailure(null);
    async function stage(file: FileSelection) {
      if (!mounted.current || !focused.current) {
        await discard(file.uri);
        return;
      }
      // Keep cleanup ownership immediately; an unmount can precede React's next effect.
      uriToDiscard.current = file.uri;
      setSelection(file);
    }
    try {
      if (kind === 'document') {
        const result = await DocumentPicker.getDocumentAsync({
          copyToCacheDirectory: true,
          multiple: false,
          type: '*/*',
          base64: false,
        });
        if (!result.canceled) {
          const file = result.assets[0];
          if (file)
            await stage({
              uri: file.uri,
              name: file.name,
              mime: file.mimeType || 'application/octet-stream',
              size: file.size,
            });
        }
      } else {
        const permission =
          kind === 'camera'
            ? await ImagePicker.requestCameraPermissionsAsync()
            : await ImagePicker.requestMediaLibraryPermissionsAsync();
        if (!mounted.current || !focused.current) return;
        if (!permission.granted) {
          await inform(
            kind === 'camera' ? 'Permitir câmera' : 'Permitir fotos',
            'Você pode permitir o acesso nos Ajustes do aparelho para selecionar esta mídia.',
          );
          return;
        }
        const result =
          kind === 'camera'
            ? await ImagePicker.launchCameraAsync({
                mediaTypes: ['images'],
                quality: 1,
                exif: false,
              })
            : await ImagePicker.launchImageLibraryAsync({
                mediaTypes: ['images', 'videos'],
                quality: 1,
                allowsMultipleSelection: false,
                exif: false,
              });
        if (!result.canceled) {
          const asset = result.assets[0];
          if (asset)
            await stage({
              uri: asset.uri,
              name: asset.fileName ?? (asset.type === 'video' ? 'video.mov' : 'foto.jpg'),
              mime: asset.mimeType ?? (asset.type === 'video' ? 'video/quicktime' : 'image/jpeg'),
              size: asset.fileSize,
            });
        }
      }
    } catch (reason) {
      if (mounted.current && focused.current)
        await inform('Não foi possível selecionar a mídia', failureMessage(reason));
    } finally {
      selecting.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  async function start() {
    try {
      await sensitiveCacheWork.run(startRecording);
    } catch (reason) {
      if (mounted.current) setFailure(failureMessage(reason));
    }
  }
  async function startRecording() {
    if (!media || busy || disabled || recording.current) return;
    setBusy(true);
    setFailure(null);
    try {
      const permission = await AudioModule.requestRecordingPermissionsAsync();
      if (!mounted.current || !focused.current) return;
      if (!permission.granted) {
        await inform(
          'Permitir microfone',
          'Permita o microfone nos Ajustes do aparelho para gravar uma mensagem de voz.',
        );
        return;
      }
      pauseAudioPlayback();
      await setAudioModeAsync({
        playsInSilentMode: true,
        allowsRecording: true,
        shouldPlayInBackground: false,
        allowsBackgroundRecording: false,
        interruptionMode: 'doNotMix',
      });
      if (!mounted.current || !focused.current) {
        await setAudioModeAsync({ allowsRecording: false });
        return;
      }
      await recorder.prepareToRecordAsync();
      if (!mounted.current || !focused.current) {
        await recorder.stop();
        await discard(recorder.uri ?? undefined);
        await setAudioModeAsync({ allowsRecording: false });
        return;
      }
      levels.current = [];
      recorder.record();
      recording.current = true;
    } catch (reason) {
      await inform('Não foi possível gravar', failureMessage(reason));
      await setAudioModeAsync({ allowsRecording: false });
    } finally {
      setBusy(false);
    }
  }
  async function send() {
    if (!media || busy || (!selection && !clip)) return;
    setBusy(true);
    setFailure(null);
    const uri = selection?.uri ?? clip?.uri;
    try {
      if (selection) await media.sendFile({ ...selection, spaceId });
      else if (clip)
        await media.sendVoice({
          ...clip,
          spaceId,
          mime: Platform.OS === 'web' ? 'audio/webm' : 'audio/mp4',
          replyTo,
          thread,
        });
      // Rust accepted the durable mutation. Refresh failure must not send a duplicate.
      setSelection(null);
      setClip(null);
      uriToDiscard.current = undefined;
      await discard(uri);
      onSent?.();
      void store.refresh();
    } catch (reason) {
      setFailure(failureMessage(reason));
    } finally {
      setBusy(false);
    }
  }
  async function cancel() {
    if (busy) return;
    await stop();
    await discard(selection?.uri ?? clip?.uri ?? recorder.uri ?? undefined);
    setSelection(null);
    setClip(null);
    setFailure(null);
  }
  if (!media) return null;
  const isRecording = recorderState.isRecording;
  const presented = Boolean(selection || clip || isRecording);
  return (
    <>
      {!thread && replyTo === null ? (
        <IconButton
          label="Adicionar anexo"
          name={{ ios: 'plus', android: 'add', web: 'add' }}
          disabled={disabled || busy || presented}
          onPress={() => setMenu(true)}
        />
      ) : null}
      <IconButton
        label="Gravar mensagem de voz"
        name={{ ios: 'mic', android: 'mic', web: 'mic' }}
        disabled={disabled || busy || presented}
        onPress={() => {
          void start();
        }}
      />
      <Host colorScheme={scheme} seedColor={colors.accent}>
        <BottomSheet isPresented={menu} onDismiss={() => setMenu(false)}>
          <Column spacing={8}>
            <Button
              label="Fotos e vídeos"
              variant="text"
              onPress={() => {
                void pick('photo');
              }}
            />
            <Button
              label="Câmera"
              variant="text"
              onPress={() => {
                void pick('camera');
              }}
            />
            <Button
              label="Arquivo"
              variant="text"
              onPress={() => {
                void pick('document');
              }}
            />
          </Column>
        </BottomSheet>
      </Host>
      <Modal
        visible={presented}
        animationType="slide"
        presentationStyle="pageSheet"
        onRequestClose={() => {
          if (!busy) void cancel();
        }}
      >
        <ScrollView
          contentContainerStyle={{
            flexGrow: 1,
            padding: 24,
            paddingTop: Math.max(insets.top, 30),
            paddingBottom: Math.max(insets.bottom, 24),
            gap: 24,
            backgroundColor: colors.surface,
          }}
        >
          <View
            style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}
          >
            <Text
              accessibilityRole="header"
              style={{ color: colors.text, fontSize: 24, fontWeight: '600' }}
            >
              {isRecording ? 'Gravando…' : clip ? 'Mensagem de voz' : 'Enviar anexo'}
            </Text>
            <IconButton
              label="Cancelar anexo"
              name={{ ios: 'xmark', android: 'close', web: 'close' }}
              disabled={busy}
              onPress={() => {
                void cancel();
              }}
            />
          </View>
          {selection?.mime.startsWith('image/') ? (
            <Image
              cachePolicy="none"
              source={{ uri: selection.uri }}
              style={{ width: '100%', height: 320, borderRadius: 18 }}
              contentFit="contain"
            />
          ) : null}
          {selection?.mime.startsWith('video/') ? <VideoClip uri={selection.uri} /> : null}
          {selection ? (
            <>
              <Text selectable style={{ color: colors.text, fontSize: 18, lineHeight: 26 }}>
                {selection.name}
              </Text>
              <Text style={{ color: colors.secondary }}>
                {selection.size == null
                  ? selection.mime
                  : `${(selection.size / 1048576).toFixed(1)} MB · ${selection.mime}`}
              </Text>
            </>
          ) : null}
          {isRecording ? (
            <>
              <Text
                accessibilityLiveRegion="polite"
                style={{
                  color: colors.danger,
                  fontSize: 44,
                  textAlign: 'center',
                  fontVariant: ['tabular-nums'],
                }}
              >
                {clipTime(recorderState.durationMillis / 1000)}
              </Text>
              <NativeButton
                label="Parar gravação"
                onPress={() => {
                  void stop();
                }}
              />
            </>
          ) : clip ? (
            <AudioClip uri={clip.uri} durationMs={clip.durationMs} levels={clip.levels} />
          ) : null}
          {failure ? (
            <Text
              accessibilityRole="alert"
              selectable
              style={{ color: colors.danger, lineHeight: 22 }}
            >
              {failure}
            </Text>
          ) : null}
          {!isRecording ? (
            <NativeButton
              label={busy ? 'Enviando…' : 'Enviar'}
              disabled={busy || Boolean(selection?.size && selection.size > MAX_SELECTED_BYTES)}
              onPress={() => {
                void send();
              }}
            />
          ) : null}
          {selection?.size && selection.size > MAX_SELECTED_BYTES ? (
            <Text accessibilityRole="alert" style={{ color: colors.danger }}>
              Escolha um arquivo de até 64 MB.
            </Text>
          ) : null}
          <Text style={{ color: colors.secondary, fontSize: 13, lineHeight: 20 }}>
            Revise antes de enviar. O arquivo fica salvo nesta conversa e sincroniza quando houver
            conexão.
          </Text>
        </ScrollView>
      </Modal>
    </>
  );
}
