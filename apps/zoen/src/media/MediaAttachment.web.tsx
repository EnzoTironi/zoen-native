import { useEffect, useRef, useState } from 'react';
import { ActivityIndicator, Modal, Pressable, ScrollView, Text, View } from 'react-native';
import { Image } from 'expo-image';
import type { TimelineEntry } from '../core/types';
import type { FileDto } from '../core/generated/FileDto';
import { useClient } from '../core/provider';
import { useTheme } from '../theme';
import { Icon, IconButton, NativeButton } from '../components/primitives';
import type { LocalMediaFile } from './types';
import { AudioClip } from './AudioClip';

type Props = {
  itemId: string;
  version?: number;
  title?: string;
  voice?: TimelineEntry['voice'];
  expectedHash?: string;
};
const safeImages = new Set(['image/png', 'image/jpeg', 'image/webp', 'image/gif', 'image/avif']);
const failure = (reason: unknown) =>
  reason instanceof Error ? reason.message : 'Não foi possível abrir este arquivo.';

export function FileAttachment(props: Props) {
  return (
    <WebFile
      key={`${props.itemId}:${props.version ?? 'latest'}:${props.expectedHash ?? ''}`}
      {...props}
    />
  );
}

function WebFile({ itemId, version, title, voice, expectedHash }: Props) {
  const { store, snapshot } = useClient();
  const { colors } = useTheme();
  const media = store.driver.media;
  const [info, setInfo] = useState<FileDto | null>(null);
  const [local, setLocal] = useState<LocalMediaFile | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState(false);
  const [textPreview, setTextPreview] = useState<string | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    let current = true;
    if (media)
      void media
        .fileInfo(itemId, version)
        .then((value) => {
          if (current) setInfo(value);
        })
        .catch((reason: unknown) => {
          if (current) setError(failure(reason));
        });
    return () => {
      current = false;
    };
  }, [media, itemId, version, snapshot]);
  useEffect(() => {
    if (!media || !local) return;
    return () => {
      void media.releaseLocalFile(local.uri);
    };
  }, [media, local]);

  async function open() {
    if (!media || busy) return;
    setBusy(true);
    setError(null);
    let acquired: LocalMediaFile | null = null;
    try {
      acquired = local ?? (await media.materializeFile(itemId, version));
      if (!acquired)
        throw new Error(
          'Este arquivo ainda está chegando. Tente novamente quando estiver conectado.',
        );
      if (expectedHash && acquired.info.sha256 !== expectedHash)
        throw new Error('O áudio foi atualizado. Abra esta mensagem novamente.');
      if (!mounted.current) return;
      if (acquired.info.mime.startsWith('text/') && Number(acquired.info.bytes) <= 1024 * 1024) {
        const response = await fetch(acquired.uri);
        const text = await response.text();
        if (!mounted.current) return;
        setTextPreview(text);
      }
      setLocal(acquired);
      acquired = null;
      if (!voice) setPreview(true);
    } catch (reason) {
      if (mounted.current) setError(failure(reason));
    } finally {
      if (acquired && acquired !== local) await media.releaseLocalFile(acquired.uri);
      if (mounted.current) setBusy(false);
    }
  }
  function download() {
    if (!local) return;
    const anchor = document.createElement('a');
    anchor.href = local.uri;
    anchor.download = local.info.name;
    anchor.rel = 'noopener';
    anchor.click();
  }
  async function share() {
    if (!local) return;
    try {
      const response = await fetch(local.uri);
      const file = new globalThis.File([await response.blob()], local.info.name, {
        type: local.info.mime,
      });
      if (navigator.canShare?.({ files: [file] }))
        await navigator.share({ files: [file], title: local.info.name });
      else download();
    } catch (reason) {
      if (!(reason instanceof DOMException && reason.name === 'AbortError'))
        setError(failure(reason));
    }
  }
  return (
    <View
      style={{
        minWidth: 210,
        padding: 12,
        borderRadius: 15,
        gap: 10,
        backgroundColor: colors.surface,
      }}
    >
      {voice && local ? (
        <AudioClip uri={local.uri} durationMs={Number(voice.ms)} levels={voice.levels} />
      ) : (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={
            voice ? 'Carregar mensagem de voz' : `Abrir ${title ?? info?.name ?? 'arquivo'}`
          }
          onPress={() => {
            void open();
          }}
          disabled={busy || !media}
          style={{ minHeight: 52, flexDirection: 'row', gap: 12, alignItems: 'center' }}
        >
          {busy ? (
            <ActivityIndicator color={colors.accent} />
          ) : (
            <Icon
              name={
                voice
                  ? { ios: 'play.circle', android: 'play_circle', web: 'play_circle' }
                  : { ios: 'doc', android: 'description', web: 'description' }
              }
              color={colors.accent}
              size={30}
            />
          )}
          <View style={{ flex: 1, gap: 5 }}>
            <Text style={{ color: colors.text, fontWeight: '600', fontSize: 16 }} numberOfLines={2}>
              {voice ? 'Mensagem de voz' : (title ?? info?.name ?? 'Arquivo')}
            </Text>
            <Text style={{ color: colors.secondary, fontSize: 12 }}>
              {info ? `${(Number(info.bytes) / 1024).toFixed(1)} KB · ${info.mime}` : 'Carregando…'}
            </Text>
          </View>
        </Pressable>
      )}
      {voice?.transcript ? (
        <Text selectable style={{ color: colors.secondary, fontSize: 14, lineHeight: 21 }}>
          {voice.transcript}
        </Text>
      ) : null}
      {local ? (
        <View style={{ flexDirection: 'row', gap: 16 }}>
          <Pressable
            accessibilityRole="button"
            onPress={download}
            style={{ minHeight: 44, justifyContent: 'center' }}
          >
            <Text style={{ color: colors.accent, fontWeight: '600', fontSize: 13 }}>Baixar</Text>
          </Pressable>
          <Pressable
            accessibilityRole="button"
            onPress={() => {
              void share();
            }}
            style={{ minHeight: 44, justifyContent: 'center' }}
          >
            <Text style={{ color: colors.accent, fontWeight: '600', fontSize: 13 }}>
              Compartilhar
            </Text>
          </Pressable>
        </View>
      ) : null}
      {error ? (
        <Text
          accessibilityRole="alert"
          selectable
          style={{ color: colors.danger, fontSize: 13, lineHeight: 20 }}
        >
          {error}
        </Text>
      ) : null}
      <Modal
        visible={preview && Boolean(local)}
        animationType="fade"
        transparent
        onRequestClose={() => setPreview(false)}
      >
        <View
          style={{
            flex: 1,
            alignItems: 'center',
            justifyContent: 'center',
            padding: 20,
            backgroundColor: '#00000099',
          }}
        >
          <View
            style={{
              width: '100%',
              maxWidth: 960,
              maxHeight: '95%',
              backgroundColor: colors.surface,
              borderRadius: 24,
              padding: 20,
              gap: 16,
            }}
          >
            <View
              style={{
                flexDirection: 'row',
                alignItems: 'center',
                justifyContent: 'space-between',
                gap: 20,
              }}
            >
              <Text
                accessibilityRole="header"
                numberOfLines={2}
                style={{ flex: 1, color: colors.text, fontSize: 20, fontWeight: '600' }}
              >
                {local?.info.name}
              </Text>
              <IconButton
                label="Fechar visualização"
                name={{ ios: 'xmark', android: 'close', web: 'close' }}
                onPress={() => setPreview(false)}
              />
            </View>
            {local && safeImages.has(local.info.mime) ? (
              <Image
                cachePolicy="none"
                source={{ uri: local.uri }}
                style={{ width: '100%', height: 480 }}
                contentFit="contain"
              />
            ) : local?.info.mime.startsWith('video/') ? (
              // Shared files have no caption track unless one was supplied by their author.
              // oxlint-disable-next-line jsx-a11y/media-has-caption
              <video
                controls
                aria-label={local.info.name}
                src={local.uri}
                style={{ display: 'block', maxWidth: '100%', maxHeight: '65vh' }}
              />
            ) : local?.info.mime.startsWith('audio/') ? (
              <AudioClip uri={local.uri} />
            ) : local?.info.mime === 'application/pdf' ? (
              <iframe
                title={local.info.name}
                sandbox="allow-same-origin"
                src={local.uri}
                style={{ width: '100%', height: '65vh', border: 0 }}
              />
            ) : textPreview !== null ? (
              <ScrollView style={{ maxHeight: 480 }}>
                <Text
                  selectable
                  style={{ color: colors.text, fontFamily: 'monospace', lineHeight: 22 }}
                >
                  {textPreview}
                </Text>
              </ScrollView>
            ) : (
              <Text style={{ color: colors.secondary, lineHeight: 24 }}>
                Baixe este arquivo para abrir em um app compatível.
              </Text>
            )}
            <View style={{ flexDirection: 'row', gap: 12 }}>
              <NativeButton label="Baixar arquivo" onPress={download} />
              <NativeButton
                label="Compartilhar"
                secondary
                onPress={() => {
                  void share();
                }}
              />
            </View>
          </View>
        </View>
      </Modal>
    </View>
  );
}

export function MessageMedia({ entry }: { entry: TimelineEntry }) {
  if (entry.deleted) return null;
  if (entry.voice)
    return (
      <FileAttachment
        itemId={entry.voice.item_id}
        voice={entry.voice}
        expectedHash={entry.voice.sha256}
      />
    );
  const card = 'Message' in entry.kind ? entry.kind.Message.card : null;
  return card?.kind_id === 'file' ? (
    <FileAttachment itemId={card.item_id} version={card.version} title={card.title} />
  ) : null;
}
