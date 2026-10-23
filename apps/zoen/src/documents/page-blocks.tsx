import { t } from '../i18n/index.ts';
import { Pressable, Text, View } from 'react-native';
import { PageImageBlock } from './page-image-block';
import { usePageImageItems } from './page-image-items';
import type { PageDto } from '../core/types';
import type { PageBlockDto } from '../core/generated/PageBlockDto';
import { Icon, IconButton } from '../components/primitives';
import { RichText } from '../components/rich-text';
import { useTheme } from '../theme';

export function PageBlocks({
  blocks,
  page,
  editable = false,
  busy = false,
  organizing = false,
  onEdit,
  onToggle,
  onMove,
  onDelete,
}: {
  blocks: readonly PageBlockDto[];
  page?: PageDto;
  editable?: boolean;
  busy?: boolean;
  organizing?: boolean;
  onEdit?: (block: PageBlockDto) => void;
  onToggle?: (block: PageBlockDto) => void;
  onMove?: (block: PageBlockDto, direction: -1 | 1) => void;
  onDelete?: (block: PageBlockDto) => void;
}) {
  const { colors } = useTheme();
  const images = usePageImageItems(
    page,
    blocks.some((block) => block.kind === 'image'),
  );
  return (
    <View style={{ gap: 10 }}>
      {blocks.map((block, index) => (
        <View key={block.id} style={{ gap: 6 }}>
          <View
            style={{
              flexDirection: 'row',
              alignItems: 'flex-start',
              gap: 8,
              paddingVertical: block.kind === 'heading' ? 10 : 3,
              paddingLeft: Math.min(block.indent, 6) * 16,
              borderLeftWidth: block.kind === 'quote' ? 3 : 0,
              borderLeftColor: colors.accent,
            }}
          >
            {block.kind === 'task' ? (
              <Pressable
                disabled={!editable || busy}
                accessibilityRole="checkbox"
                accessibilityLabel={block.text || t('documents.untitledTask')}
                aria-checked={block.checked}
                onPress={() => onToggle?.(block)}
                style={{
                  minWidth: 44,
                  minHeight: 44,
                  alignItems: 'center',
                  justifyContent: 'flex-start',
                  paddingTop: 3,
                }}
              >
                <Icon
                  name={{
                    ios: block.checked ? 'checkmark.circle.fill' : 'circle',
                    android: block.checked ? 'check_circle' : 'radio_button_unchecked',
                    web: block.checked ? 'check_circle' : 'radio_button_unchecked',
                  }}
                  color={block.checked ? colors.accent : colors.tertiary}
                />
              </Pressable>
            ) : block.kind === 'bullet' ? (
              <Text style={{ color: colors.accent, lineHeight: 27 }}>•</Text>
            ) : block.kind === 'numbered' ? (
              <Text style={{ color: colors.secondary, lineHeight: 27 }}>{block.number}.</Text>
            ) : null}
            {block.kind === 'image' ? (
              <View style={{ flex: 1 }}>
                <PageImageBlock
                  page={page}
                  block={block}
                  items={images.items}
                  active={images.active}
                  binding={images.key}
                  isCurrent={images.current}
                  loading={images.loading}
                  error={images.error}
                  retry={images.retry}
                />
                {editable ? (
                  <IconButton
                    label={t('documents.editBlockLabel', {
                      text: block.alt || t('documents.image'),
                    })}
                    name={{ ios: 'pencil', android: 'edit', web: 'edit' }}
                    disabled={busy}
                    onPress={() => onEdit?.(block)}
                  />
                ) : null}
              </View>
            ) : (
              <Pressable
                accessibilityRole={editable ? 'button' : undefined}
                accessibilityLabel={
                  editable
                    ? block.kind === 'heading'
                      ? t('documents.editHeadingLabel', {
                          text: block.text || t('documents.emptyBlock'),
                        })
                      : t('documents.editBlockLabel', {
                          text: block.text || t('documents.emptyBlock'),
                        })
                    : undefined
                }
                accessibilityHint={editable ? t('documents.opensTheTextEditor') : undefined}
                disabled={!editable || busy}
                onPress={() => onEdit?.(block)}
                style={{ flex: 1, minHeight: 32 }}
              >
                {block.kind === 'divider' ? (
                  <View style={{ height: 1, marginVertical: 14, backgroundColor: colors.line }} />
                ) : (
                  <>
                    <RichText
                      text={block.text || (editable ? t('documents.emptyBlock') : '')}
                      spans={block.spans}
                      style={{
                        color: !block.text && editable ? colors.tertiary : colors.text,
                        fontSize:
                          block.kind === 'heading'
                            ? block.level === 1
                              ? 34
                              : block.level === 2
                                ? 26
                                : 21
                            : 17,
                        lineHeight: block.kind === 'heading' ? (block.level === 1 ? 42 : 34) : 27,
                        fontWeight: block.kind === 'heading' ? '600' : '400',
                        fontFamily:
                          block.kind === 'code' || block.kind === 'raw'
                            ? process.env.EXPO_OS === 'ios'
                              ? 'Menlo'
                              : 'monospace'
                            : undefined,
                      }}
                    />
                  </>
                )}
              </Pressable>
            )}
          </View>
          {organizing && editable ? (
            <View
              style={{
                flexDirection: 'row',
                justifyContent: 'flex-end',
                alignItems: 'center',
                gap: 8,
                borderBottomWidth: 1,
                borderBottomColor: colors.line,
              }}
            >
              <Text style={{ flex: 1, color: colors.secondary, fontSize: 12 }}>
                {t('documents.blockPosition', { position: index + 1, total: blocks.length })}
              </Text>
              <IconButton
                label={t('documents.moveBlockUp', { position: index + 1 })}
                name={{ ios: 'arrow.up', android: 'arrow_upward', web: 'arrow_upward' }}
                disabled={busy || index === 0}
                onPress={() => onMove?.(block, -1)}
              />
              <IconButton
                label={t('documents.moveBlockDown', { position: index + 1 })}
                name={{ ios: 'arrow.down', android: 'arrow_downward', web: 'arrow_downward' }}
                disabled={busy || index === blocks.length - 1}
                onPress={() => onMove?.(block, 1)}
              />
              <IconButton
                label={t('documents.deleteBlockPosition', { position: index + 1 })}
                name={{ ios: 'trash', android: 'delete', web: 'delete' }}
                disabled={busy}
                onPress={() => onDelete?.(block)}
              />
            </View>
          ) : null}
        </View>
      ))}
    </View>
  );
}
