import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { ScrollView, Text, View } from 'react-native';
import { useClient } from '../../../core/provider';
import { useCoreQuery } from '../../../core/query';
import type { PlanPageDto } from '../../../core/types';
import { money, parseMoneyInput } from '../../../core/presentation';
import { EmptyState, Field, Loading, NativeButton, Surface } from '../../../components/primitives';
import { useDialogs } from '../../../components/dialogs';
import { useTheme } from '../../../theme';
import { planAmountInput } from '../../../documents/editing';
import { useDirtyEditor } from '../../../documents/use-dirty-editor';
import { rememberPlanUndo } from '../../../documents/plan-undo';

function findLine(plan: PlanPageDto, id: string | undefined) {
  return plan.blocks.find(
    (block) =>
      typeof block.content === 'object' &&
      'Line' in block.content &&
      block.content.Line.legacy_id === id,
  );
}

function Editor({
  initial,
  lineId,
  sectionIndex,
}: {
  initial: PlanPageDto;
  lineId?: string;
  sectionIndex: number;
}) {
  const { store } = useClient();
  const { colors } = useTheme();
  const dialogs = useDialogs();
  // The binding advances only after the person explicitly reviews a newer plan.
  const [observed, setObserved] = useState(initial);
  const [review, setReview] = useState<PlanPageDto | null>(null);
  const original = findLine(initial, lineId);
  const initialAmount =
    original && typeof original.content === 'object' && 'Line' in original.content
      ? planAmountInput(original.content.Line.cost_cents)
      : '0,00';
  const [text, setText] = useState(original?.text ?? '');
  const [amount, setAmount] = useState(initialAmount);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dirty = text !== (original?.text ?? '') || amount !== initialAmount;
  const finish = useDirtyEditor(dirty, busy);
  const sections = observed.blocks.filter((block) => block.content === 'Section');
  const editable =
    observed.ready &&
    observed.legacy_edit_admitted &&
    (lineId ? !!findLine(observed, lineId) : sectionIndex >= 0 && sectionIndex < sections.length);
  if (lineId && !original)
    return (
      <EmptyState
        title="Esta linha não está mais no plano"
        detail="Volte ao plano para abrir a versão atual."
      />
    );

  async function save() {
    if (busy || !editable) return;
    setBusy(true);
    setError(null);
    try {
      const cost_cents = parseMoneyInput(amount);
      if (!text.trim()) throw new Error('Escreva o texto desta linha.');
      const result = await store.request(
        lineId
          ? {
              op: 'edit_plan_line',
              item_id: observed.item_id,
              line_id: lineId,
              text: text.trim(),
              cost_cents,
              observed_hash: observed.content_hash,
            }
          : {
              op: 'add_plan_line',
              item_id: observed.item_id,
              section_index: sectionIndex,
              text: text.trim(),
              cost_cents,
              observed_hash: observed.content_hash,
            },
      );
      rememberPlanUndo(store, result.undo);
      finish();
      router.back();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : 'Não foi possível guardar esta linha.');
    } finally {
      setBusy(false);
    }
  }
  async function remove() {
    if (busy || !editable || !lineId) return;
    if (
      !(await dialogs.confirm({
        title: 'Remover esta linha?',
        message: `${original?.text ?? 'Esta linha'}\n\nA versão anterior continuará no histórico.`,
        confirmLabel: 'Remover linha',
        destructive: true,
      }))
    )
      return;
    setBusy(true);
    setError(null);
    try {
      const result = await store.request({
        op: 'remove_plan_line',
        item_id: observed.item_id,
        line_id: lineId,
        observed_hash: observed.content_hash,
      });
      rememberPlanUndo(store, result.undo);
      finish();
      router.back();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : 'Não foi possível remover esta linha.');
    } finally {
      setBusy(false);
    }
  }
  async function loadCurrent() {
    if (busy) return;
    setBusy(true);
    try {
      setReview(await store.driver.request({ op: 'plan_page', item_id: observed.item_id }));
    } catch (failure) {
      setError(
        failure instanceof Error ? failure.message : 'Não foi possível atualizar este plano.',
      );
    } finally {
      setBusy(false);
    }
  }
  const reviewLine = review ? findLine(review, lineId) : undefined;
  return (
    <ScrollView
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{
        padding: 24,
        paddingBottom: 48,
        gap: 22,
        maxWidth: 720,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen options={{ title: lineId ? 'Editar linha' : 'Nova linha' }} />
      <Text style={{ color: colors.secondary, fontSize: 13 }}>
        PLANO · VERSÃO OBSERVADA {observed.version}
      </Text>
      {!lineId ? (
        <Text style={{ color: colors.accent, fontWeight: '600' }}>
          {sections[sectionIndex]?.text ?? 'Seção indisponível'}
        </Text>
      ) : null}
      <Field
        label="Texto da linha"
        value={text}
        onChangeText={setText}
        editable={!busy && editable}
        multiline
        autoFocus
        placeholder="O que precisa acontecer?"
      />
      <Field
        label="Custo em reais"
        value={amount}
        onChangeText={setAmount}
        editable={!busy && editable}
        keyboardType="decimal-pad"
        placeholder="0,00"
      />
      <Text style={{ color: colors.secondary, fontSize: 13, lineHeight: 20 }}>
        Use vírgula para os centavos. Os valores são guardados exatamente, sem arredondamento.
      </Text>
      {!editable ? (
        <Text accessibilityRole="alert" style={{ color: colors.secondary }}>
          O plano está disponível somente para leitura ou esta linha mudou. Sua edição continua
          neste editor.
        </Text>
      ) : null}
      {error ? (
        <Surface>
          <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
            {error}
          </Text>
          <NativeButton
            secondary
            label="Rever a versão atual sem perder minha edição"
            disabled={busy}
            onPress={() => {
              void loadCurrent();
            }}
          />
        </Surface>
      ) : null}
      {review ? (
        <Surface>
          <Text
            accessibilityRole="header"
            style={{ color: colors.text, fontSize: 18, fontWeight: '600' }}
          >
            Versão atual: {review.version}
          </Text>
          {lineId ? (
            reviewLine && typeof reviewLine.content === 'object' && 'Line' in reviewLine.content ? (
              <>
                <Text selectable style={{ color: colors.text }}>
                  {reviewLine.text}
                </Text>
                <Text selectable style={{ color: colors.secondary }}>
                  {money(reviewLine.content.Line.cost_cents)} ·{' '}
                  {reviewLine.content.Line.done ? 'Concluída' : 'Pendente'}
                </Text>
              </>
            ) : (
              <Text style={{ color: colors.danger }}>Esta linha foi removida.</Text>
            )
          ) : (
            <Text style={{ color: colors.text }}>
              Adicionar na seção:{' '}
              {review.blocks.filter((block) => block.content === 'Section')[sectionIndex]?.text ??
                'Seção indisponível'}
            </Text>
          )}
          <Text style={{ color: colors.secondary, lineHeight: 21 }}>
            Seu texto e custo permanecem no editor. Use esta versão como base somente depois de
            conferir as mudanças.
          </Text>
          <NativeButton
            secondary
            label="Usar esta versão como base"
            disabled={
              busy ||
              !review.ready ||
              !review.legacy_edit_admitted ||
              (lineId
                ? !reviewLine
                : !review.blocks.filter((block) => block.content === 'Section')[sectionIndex])
            }
            onPress={() => {
              setObserved(review);
              setReview(null);
              setError(null);
            }}
          />
        </Surface>
      ) : null}
      <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 12 }}>
        <NativeButton
          label={busy ? 'Guardando…' : lineId ? 'Guardar alteração' : 'Adicionar ao plano'}
          disabled={busy || !editable || !text.trim() || (lineId ? !dirty : false)}
          onPress={() => {
            void save();
          }}
        />
        {lineId ? (
          <NativeButton
            secondary
            label="Remover linha"
            disabled={busy || !editable}
            onPress={() => {
              void remove();
            }}
          />
        ) : null}
      </View>
    </ScrollView>
  );
}

export default function PlanLine() {
  const { id, line, section } = useLocalSearchParams<{
    id: string;
    line?: string;
    section?: string;
  }>();
  const { data: plan, error, retry } = useCoreQuery({ op: 'plan_page', item_id: id });
  const sectionIndex = section && /^\d+$/.test(section) ? Number(section) : -1;
  if (!plan)
    return error ? (
      <EmptyState title="Não foi possível abrir o plano" detail={error}>
        <NativeButton label="Tentar novamente" onPress={retry} />
      </EmptyState>
    ) : (
      <Loading />
    );
  return (
    <Editor
      key={`${id}:${line ?? section}`}
      initial={plan}
      lineId={line}
      sectionIndex={sectionIndex}
    />
  );
}
