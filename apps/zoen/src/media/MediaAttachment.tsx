import { useCallback, useEffect, useRef, useState } from 'react';
import { ActivityIndicator, Pressable, Text, View } from 'react-native';
import { File } from 'expo-file-system';
import * as Sharing from 'expo-sharing';
import { Image } from 'expo-image';
import type { MediaDriver, LocalMediaFile } from './types';
import type { FileDto } from '../core/generated/FileDto';
import type { TimelineEntry } from '../core/types';
import { useClient } from '../core/provider';
import { useTheme } from '../theme';
import { Icon } from '../components/primitives';
import { AudioClip } from './AudioClip';

function sizeLabel(bytes: string) {
  const value = Number(bytes);
  return value < 1024
    ? `${value} B`
    : value < 1024 * 1024
      ? `${(value / 1024).toFixed(1)} KB`
      : `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

type FileAttachmentProps = {
  itemId: string;
  version?: number;
  title?: string;
  voice?: TimelineEntry['voice'];
  expectedHash?: string;
};

/** Version/hash identity owns the player and releases its prior decrypted copy. */
export function FileAttachment(props: FileAttachmentProps) {
  return (
    <FileAttachmentContent
      key={`${props.itemId}:${props.version ?? 'latest'}:${props.expectedHash ?? ''}`}
      {...props}
    />
  );
}

function FileAttachmentContent({
  itemId,
  version,
  title,
  voice,
  expectedHash,
}: FileAttachmentProps) {
  const { store, snapshot } = useClient();
  const driver = store.driver.media;
  const { colors } = useTheme();
  const [info, setInfo] = useState<FileDto | null>(null);
  const [local, setLocal] = useState<LocalMediaFile | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  // A sync change updates readiness; it never restores a historical version.
  useEffect(() => {
    let active = true;
    if (driver)
      void driver
        .fileInfo(itemId, version)
        .then((value) => {
          if (active) setInfo(value);
        })
        .catch((reason: unknown) => {
          if (active)
            setError(
              reason instanceof Error ? reason.message : 'Não foi possível abrir este arquivo.',
            );
        });
    return () => {
      active = false;
    };
  }, [driver, itemId, version, expectedHash, snapshot]);
  useEffect(() => {
    if (!local || !driver) return;
    return () => {
      void driver.releaseLocalFile(local.uri);
    };
  }, [driver, local]);

  const materialize = useCallback(
    async (media: MediaDriver) => {
      const file = await media.materializeFile(itemId, version);
      if (!file)
        throw new Error(
          'Este arquivo ainda está chegando. Tente novamente quando a sincronização terminar.',
        );
      if (expectedHash && file.info.sha256 !== expectedHash) {
        await media.releaseLocalFile(file.uri);
        throw new Error('O áudio foi atualizado. Abra a mensagem novamente.');
      }
      return file;
    },
    [itemId, version, expectedHash],
  );
  async function action(kind: 'open' | 'share') {
    if (!driver || busy) return;
    setBusy(true);
    setError(null);
    let file: LocalMediaFile | null = null;
    try {
      file = await materialize(driver);
      if (!mounted.current) return;
      if (voice && kind === 'open') {
        setLocal(file);
        file = null;
        return;
      }
      if (kind === 'share') {
        if (!(await Sharing.isAvailableAsync()))
          throw new Error('O compartilhamento não está disponível neste aparelho.');
        await Sharing.shareAsync(file.uri, {
          mimeType: file.info.mime,
          dialogTitle: file.info.name,
        });
      } else {
        const preview = new File(file.uri);
        if (!(await preview.canPreview({ mimeType: file.info.mime })))
          throw new Error(
            'Este formato não tem visualização no aparelho. Use Compartilhar para abrir em outro app.',
          );
        await preview.preview({ title: file.info.name, mimeType: file.info.mime });
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : 'Não foi possível abrir este arquivo.');
    } finally {
      if (file) await driver.releaseLocalFile(file.uri);
      setBusy(false);
    }
  }
  return (
    <View
      style={{
        gap: 8,
        minWidth: 210,
        padding: 12,
        borderRadius: 15,
        backgroundColor: colors.surface,
      }}
    >
      {voice && local ? (
        <AudioClip
          key={`${itemId}:${local.info.sha256}`}
          uri={local.uri}
          durationMs={Number(voice.ms)}
          levels={voice.levels}
        />
      ) : (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={
            voice ? 'Carregar mensagem de voz' : `Abrir ${title ?? info?.name ?? 'arquivo'}`
          }
          disabled={busy || !driver}
          onPress={() => {
            void action('open');
          }}
          style={{ flexDirection: 'row', gap: 12, alignItems: 'center', minHeight: 52 }}
        >
          {busy ? (
            <ActivityIndicator color={colors.accent} />
          ) : (
            <Icon
              name={
                voice
                  ? { ios: 'play.circle.fill', android: 'play_circle', web: 'play_circle' }
                  : { ios: 'doc.fill', android: 'description', web: 'description' }
              }
              size={30}
              color={colors.accent}
            />
          )}
          <View style={{ flex: 1, gap: 4 }}>
            <Text numberOfLines={2} style={{ color: colors.text, fontSize: 16, fontWeight: '600' }}>
              {voice ? 'Mensagem de voz' : (title ?? info?.name ?? 'Arquivo')}
            </Text>
            <Text style={{ color: colors.secondary, fontSize: 12 }}>
              {info
                ? `${sizeLabel(info.bytes)} · ${info.ready ? 'Pronto para abrir' : `${info.chunks_here}/${info.chunks} partes`}`
                : 'Carregando…'}
            </Text>
          </View>
        </Pressable>
      )}
      {local?.info.mime.startsWith('image/') ? (
        <Image
          source={{ uri: local.uri }}
          style={{ width: '100%', height: 180, borderRadius: 12 }}
          contentFit="contain"
        />
      ) : null}
      {voice?.transcript ? (
        <Text selectable style={{ color: colors.secondary, fontSize: 14, lineHeight: 20 }}>
          {voice.transcript}
        </Text>
      ) : null}
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={`Compartilhar ${voice ? 'áudio' : (title ?? info?.name ?? 'arquivo')}`}
        disabled={busy || !driver}
        onPress={() => {
          void action('share');
        }}
        style={{ minHeight: 36, justifyContent: 'center' }}
      >
        <Text style={{ color: colors.accent, fontSize: 13, fontWeight: '600' }}>Compartilhar</Text>
      </Pressable>
      {error ? (
        <Text
          accessibilityRole="alert"
          style={{ color: colors.danger, fontSize: 13, lineHeight: 19 }}
        >
          {error}
        </Text>
      ) : null}
    </View>
  );
}

export function MessageMedia({ entry }: { entry: TimelineEntry }) {
  if (entry.deleted) return null;
  if (entry.voice)
    return (
      <FileAttachment
        key={`${entry.voice.item_id}:${entry.voice.sha256}`}
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
