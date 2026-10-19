import { t } from '../../../i18n/index.ts';
import { useState } from 'react';
import { Stack, router, useLocalSearchParams } from 'expo-router';
import { ScrollView, Text, View } from 'react-native';
import { useClient } from '../../../core/provider';
import { useCoreQuery } from '../../../core/query';
import type { PlanPageDto } from '../../../core/types';
import { money, parsePlanMoneyInput } from '../../../core/presentation';
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
      : planAmountInput('0');
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
        title={t('documents.thisLineIsNoLongerInThePlan')}
        detail={t('documents.goBackToThePlanToOpenThe')}
      />
    );

  async function save() {
    if (busy || !editable) return;
    setBusy(true);
    setError(null);
    try {
      const cost_cents = parsePlanMoneyInput(amount);
      if (!text.trim()) throw new Error(t('documents.enterTheTextForThisLine'));
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
      setError(failure instanceof Error ? failure.message : t('documents.thisLineCouldNotBeSaved'));
    } finally {
      setBusy(false);
    }
  }
  async function remove() {
    if (busy || !editable || !lineId) return;
    if (
      !(await dialogs.confirm({
        title: t('documents.removeThisLine'),
        message: t('documents.removeLineConfirmation', {
          text: original?.text ?? t('documents.thisLine'),
        }),
        confirmLabel: t('documents.removeLine'),
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
      setError(
        failure instanceof Error ? failure.message : t('documents.thisLineCouldNotBeRemoved'),
      );
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
        failure instanceof Error ? failure.message : t('documents.thisPlanCouldNotBeUpdated'),
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
      <Stack.Screen
        options={{ title: lineId ? t('documents.editLine') : t('documents.newLine') }}
      />
      <Text style={{ color: colors.secondary, fontSize: 13 }}>
        {t('documents.planObservedVersion', { version: observed.version })}
      </Text>
      {!lineId ? (
        <Text style={{ color: colors.accent, fontWeight: '600' }}>
          {sections[sectionIndex]?.text ?? t('documents.sectionUnavailable')}
        </Text>
      ) : null}
      <Field
        label={t('documents.lineText')}
        value={text}
        onChangeText={setText}
        editable={!busy && editable}
        multiline
        autoFocus
        placeholder={t('documents.whatNeedsToHappen')}
      />
      <Field
        label={t('documents.costInBrazilianReais')}
        value={amount}
        onChangeText={setAmount}
        editable={!busy && editable}
        keyboardType="decimal-pad"
        placeholder={t('documents.amountPlaceholder')}
      />
      <Text style={{ color: colors.secondary, fontSize: 13, lineHeight: 20 }}>
        {t('documents.amountPrecisionHelp')}
      </Text>
      {!editable ? (
        <Text accessibilityRole="alert" style={{ color: colors.secondary }}>
          {t('documents.thePlanIsReadOnlyOrThisLine')}
        </Text>
      ) : null}
      {error ? (
        <Surface>
          <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
            {error}
          </Text>
          <NativeButton
            secondary
            label={t('documents.reviewTheCurrentVersionWithoutLosingMyEdit')}
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
            {t('documents.currentVersion', { version: review.version })}
          </Text>
          {lineId ? (
            reviewLine && typeof reviewLine.content === 'object' && 'Line' in reviewLine.content ? (
              <>
                <Text selectable style={{ color: colors.text }}>
                  {reviewLine.text}
                </Text>
                <Text selectable style={{ color: colors.secondary }}>
                  {money(reviewLine.content.Line.cost_cents)} ·{' '}
                  {reviewLine.content.Line.done ? t('documents.done') : t('documents.pending')}
                </Text>
              </>
            ) : (
              <Text style={{ color: colors.danger }}>{t('documents.thisLineHasBeenRemoved')}</Text>
            )
          ) : (
            <Text style={{ color: colors.text }}>
              {t('documents.addToSection', {
                section:
                  review.blocks.filter((block) => block.content === 'Section')[sectionIndex]
                    ?.text ?? t('documents.sectionUnavailable'),
              })}
            </Text>
          )}
          <Text style={{ color: colors.secondary, lineHeight: 21 }}>
            {t('documents.yourTextAndCostRemainInTheEditor')}
          </Text>
          <NativeButton
            secondary
            label={t('documents.useThisVersionAsABase')}
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
          label={
            busy
              ? t('documents.saving')
              : lineId
                ? t('documents.saveChange')
                : t('documents.addToPlan')
          }
          disabled={busy || !editable || !text.trim() || (lineId ? !dirty : false)}
          onPress={() => {
            void save();
          }}
        />
        {lineId ? (
          <NativeButton
            secondary
            label={t('documents.removeLine')}
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
      <EmptyState title={t('documents.thePlanCouldNotBeOpened')} detail={error}>
        <NativeButton label={t('documents.tryAgain')} onPress={retry} />
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
