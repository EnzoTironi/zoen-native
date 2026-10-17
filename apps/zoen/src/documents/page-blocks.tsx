import { Pressable, Text, View } from 'react-native';
import type { PageBlockDto } from '../core/generated/PageBlockDto';
import { Icon, IconButton } from '../components/primitives';
import { RichText } from '../components/rich-text';
import { useTheme } from '../theme';

export function PageBlocks({
  blocks,
  editable = false,
  busy = false,
  organizing = false,
  onEdit,
  onToggle,
  onMove,
  onDelete,
}: {
  blocks: readonly PageBlockDto[];
  editable?: boolean;
  busy?: boolean;
  organizing?: boolean;
  onEdit?: (block: PageBlockDto) => void;
  onToggle?: (block: PageBlockDto) => void;
  onMove?: (block: PageBlockDto, direction: -1 | 1) => void;
  onDelete?: (block: PageBlockDto) => void;
}) {
  const { colors } = useTheme();
  return (
    <View style={{ gap: 10 }}>
      {blocks.map((block, index) => (
        <View key={block.id} style={{ gap: 6 }}>
          <Pressable
            accessibilityRole={editable ? 'button' : undefined}
            accessibilityLabel={
              editable
                ? `Editar ${block.kind === 'heading' ? 'título' : 'bloco'}: ${block.text || 'vazio'}`
                : undefined
            }
            accessibilityHint={editable ? 'Abre o editor de texto' : undefined}
            disabled={!editable || busy}
            onPress={() => onEdit?.(block)}
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
                accessibilityLabel={block.text || 'Tarefa sem título'}
                accessibilityState={{ checked: block.checked, disabled: !editable || busy }}
                onPress={(event) => {
                  event.stopPropagation();
                  onToggle?.(block);
                }}
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
            {block.kind === 'divider' ? (
              <View
                style={{ flex: 1, height: 1, marginVertical: 14, backgroundColor: colors.line }}
              />
            ) : (
              <View style={{ flex: 1, minHeight: 32 }}>
                <RichText
                  text={
                    block.kind === 'image'
                      ? block.alt || 'Imagem'
                      : block.text || (editable ? 'Bloco vazio' : '')
                  }
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
                {block.kind === 'image' && block.url ? (
                  <Text
                    selectable
                    style={{ color: colors.secondary, fontSize: 12, lineHeight: 18 }}
                  >
                    {block.url}
                  </Text>
                ) : null}
              </View>
            )}
          </Pressable>
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
                Bloco {index + 1} de {blocks.length}
              </Text>
              <IconButton
                label={`Mover bloco ${index + 1} para cima`}
                name={{ ios: 'arrow.up', android: 'arrow_upward', web: 'arrow_upward' }}
                disabled={busy || index === 0}
                onPress={() => onMove?.(block, -1)}
              />
              <IconButton
                label={`Mover bloco ${index + 1} para baixo`}
                name={{ ios: 'arrow.down', android: 'arrow_downward', web: 'arrow_downward' }}
                disabled={busy || index === blocks.length - 1}
                onPress={() => onMove?.(block, 1)}
              />
              <IconButton
                label={`Apagar bloco ${index + 1}`}
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
