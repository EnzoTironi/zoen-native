import { Pressable, Text, View } from 'react-native';
import type { PlanPageDto } from '../core/generated/PlanPageDto';
import type { PlanPageBlockDto } from '../core/generated/PlanPageBlockDto';
import type { PlanDto } from '../core/generated/PlanDto';
import { Icon, IconButton, NativeButton, Surface } from '../components/primitives';
import { RichText } from '../components/rich-text';
import { money } from '../core/presentation';
import { useTheme } from '../theme';

export function PlanBlocks({
  plan,
  busy,
  onToggle,
  onEdit,
  onAdd,
}: {
  plan: PlanPageDto;
  busy: boolean;
  onToggle: (block: PlanPageBlockDto) => void;
  onEdit: (block: PlanPageBlockDto) => void;
  onAdd: (section: number) => void;
}) {
  const { colors } = useTheme();
  let section = -1;
  return (
    <View style={{ gap: 8 }}>
      <Surface>
        <Text style={{ color: colors.secondary }}>Total do plano</Text>
        {plan.total_cents !== null ? (
          <Text
            style={{
              color: colors.text,
              fontSize: 30,
              fontWeight: '600',
              fontVariant: ['tabular-nums'],
            }}
          >
            {money(plan.total_cents)}
          </Text>
        ) : (
          <Text accessibilityRole="alert" style={{ color: colors.danger }}>
            O total ultrapassa o limite deste plano.
          </Text>
        )}
      </Surface>
      {plan.save_error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {plan.save_error}
        </Text>
      ) : null}
      <Text accessibilityLiveRegion="polite" style={{ color: colors.secondary, fontSize: 13 }}>
        {!plan.ready
          ? 'O plano ainda está sendo sincronizado.'
          : plan.unsaved
            ? 'Rascunho neste aparelho'
            : plan.pending_sync
              ? 'Aguardando sincronização'
              : `Versão ${plan.version}`}
      </Text>
      {plan.blocks.map((block) => {
        if (typeof block.content === 'object' && 'Title' in block.content)
          return block.content.Title.budget_cents === null ? null : (
            <Text key={block.id} style={{ color: colors.secondary }}>
              Orçamento: {money(block.content.Title.budget_cents)}
            </Text>
          );
        if (block.content === 'Section') {
          const index = ++section;
          return (
            <View
              key={block.id}
              style={{
                paddingTop: 22,
                gap: 10,
                flexDirection: 'row',
                alignItems: 'center',
                flexWrap: 'wrap',
              }}
            >
              <Text
                accessibilityRole="header"
                style={{ flex: 1, color: colors.text, fontSize: 22, fontWeight: '600' }}
              >
                {block.text || 'Sem título'}
              </Text>
              {plan.ready && plan.legacy_edit_admitted ? (
                <NativeButton
                  secondary
                  label="Adicionar linha"
                  disabled={busy}
                  onPress={() => onAdd(index)}
                />
              ) : null}
            </View>
          );
        }
        if (typeof block.content === 'object' && 'Line' in block.content) {
          const line = block.content.Line;
          return (
            <View
              key={block.id}
              style={{
                flexDirection: 'row',
                alignItems: 'center',
                gap: 12,
                paddingVertical: 12,
                borderBottomWidth: 1,
                borderBottomColor: colors.line,
              }}
            >
              <Pressable
                disabled={busy || !plan.legacy_edit_admitted || !plan.ready}
                accessibilityRole="checkbox"
                accessibilityLabel={block.text}
                accessibilityState={{
                  checked: line.done,
                  disabled: busy || !plan.legacy_edit_admitted || !plan.ready,
                }}
                onPress={() => onToggle(block)}
                style={{
                  minWidth: 44,
                  minHeight: 44,
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
              >
                <Icon
                  name={{
                    ios: line.done ? 'checkmark.circle.fill' : 'circle',
                    android: line.done ? 'check_circle' : 'radio_button_unchecked',
                    web: line.done ? 'check_circle' : 'radio_button_unchecked',
                  }}
                  color={line.done ? colors.accent : colors.tertiary}
                />
              </Pressable>
              <View style={{ flex: 1, gap: 5 }}>
                <RichText
                  text={block.text}
                  spans={block.spans}
                  style={{
                    color: line.done ? colors.secondary : colors.text,
                    fontSize: 17,
                    lineHeight: 24,
                    textDecorationLine: line.done ? 'line-through' : 'none',
                  }}
                />
                <Text selectable style={{ color: colors.secondary, fontVariant: ['tabular-nums'] }}>
                  {money(line.cost_cents)}
                </Text>
              </View>
              {plan.legacy_edit_admitted && plan.ready ? (
                <IconButton
                  label={`Editar linha: ${block.text}`}
                  name={{ ios: 'pencil', android: 'edit', web: 'edit' }}
                  disabled={busy}
                  onPress={() => onEdit(block)}
                />
              ) : null}
            </View>
          );
        }
        return (
          <RichText
            key={block.id}
            text={block.text}
            spans={block.spans}
            style={{ color: colors.secondary, fontSize: 17, lineHeight: 26 }}
          />
        );
      })}
    </View>
  );
}

export function PlanVersion({ plan }: { plan: PlanDto }) {
  const { colors } = useTheme();
  return (
    <View style={{ gap: 18 }}>
      <Surface>
        <Text style={{ color: colors.secondary }}>Total nesta versão</Text>
        <Text
          selectable
          style={{ color: colors.text, fontSize: 28, fontVariant: ['tabular-nums'] }}
        >
          {money(plan.total_cents)}
        </Text>
        {plan.budget_cents === null ? null : (
          <Text style={{ color: colors.secondary }}>Orçamento: {money(plan.budget_cents)}</Text>
        )}
      </Surface>
      {plan.summary ? (
        <Text selectable style={{ color: colors.text, fontSize: 17, lineHeight: 26 }}>
          {plan.summary}
        </Text>
      ) : null}
      {plan.sections.map((section, index) => (
        <View key={index} style={{ gap: 12 }}>
          <Text
            accessibilityRole="header"
            style={{ color: colors.text, fontWeight: '600', fontSize: 21 }}
          >
            {section.title}
          </Text>
          {section.lines.map((line, lineIndex) => (
            <View
              key={`${line.id}:${lineIndex}`}
              style={{ flexDirection: 'row', gap: 12, alignItems: 'flex-start' }}
            >
              <Text style={{ color: colors.accent }}>{line.done ? '✓' : '○'}</Text>
              <Text
                selectable
                style={{
                  flex: 1,
                  color: colors.text,
                  fontSize: 17,
                  lineHeight: 25,
                  textDecorationLine: line.done ? 'line-through' : 'none',
                }}
              >
                {line.text}
              </Text>
              <Text selectable style={{ color: colors.secondary }}>
                {money(line.cost_cents)}
              </Text>
            </View>
          ))}
        </View>
      ))}
    </View>
  );
}
