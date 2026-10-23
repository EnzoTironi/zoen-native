import { Text, View } from 'react-native';
import type { ItemDetail, PageDto } from '../core/types';
import type { PageBlockDto } from '../core/generated/PageBlockDto';
import { Icon, NativeButton } from '../components/primitives';
import { FileAttachment } from '../media/MediaAttachment';
import { RichText } from '../components/rich-text';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { pageImageReference } from './page-image-reference';

/** Shared by the live editor and historical Page. Only canonical File IDs reach media. */
export function PageImageBlock({
  page,
  block,
  items,
  active,
  loading,
  error,
  retry,
  binding,
  isCurrent,
  historical = false,
}: {
  page?: PageDto;
  block: PageBlockDto;
  items: readonly ItemDetail[] | null;
  active: boolean;
  loading: boolean;
  error: string | null;
  retry(): void;
  binding: string;
  isCurrent(): boolean;
  historical?: boolean;
}) {
  const { colors } = useTheme();
  const label = block.alt || t('documents.image');
  const file =
    !historical && active && page && items ? pageImageReference(page, block.url, items) : null;
  if (file)
    return (
      <FileAttachment
        key={`${binding}:${block.id}:${block.url}:${file.itemId}:${file.version}:${file.sha256}`}
        isCurrent={isCurrent}
        itemId={file.itemId}
        version={file.version}
        expectedHash={file.sha256}
        title={label}
        presentation="message"
      />
    );
  return (
    <View style={{ gap: 8, padding: 16, borderRadius: 16, backgroundColor: colors.muted }}>
      <View style={{ flexDirection: 'row', alignItems: 'center', gap: 10 }}>
        <Icon name={{ ios: 'photo', android: 'image', web: 'image' }} color={colors.tertiary} />
        <Text style={{ color: colors.text, flex: 1 }}>{label}</Text>
      </View>
      {historical || active ? (
        <Text
          accessibilityRole={!historical && error ? 'alert' : undefined}
          style={{ color: !historical && error ? colors.danger : colors.secondary }}
        >
          {historical
            ? t('documents.pageImageHistoricalUnavailable')
            : error || (loading ? t('media.loading') : t('documents.pageImageUnavailable'))}
        </Text>
      ) : null}
      {(!loading || historical) && block.url ? (
        <RichText
          text={block.url}
          spans={
            /^https?:\/\//i.test(block.url)
              ? [{ start: 0, end: block.url.length, key: 'a', value: block.url }]
              : []
          }
          style={{ fontSize: 13 }}
        />
      ) : null}
      {!historical && active && error ? (
        <NativeButton secondary label={t('documents.tryAgain')} onPress={retry} />
      ) : null}
    </View>
  );
}
