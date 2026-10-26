import { useState } from 'react';
import { Host, Switch } from '@expo/ui';
import { Text } from 'react-native';
import { Field, NativeButton, Surface } from '../components/primitives';
import { t, locale, formatLocale } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { OwnerPolicyContext } from '../core/generated/OwnerPolicyContext';
import { validContext } from '../owner-policy/values';
import { policyAmount } from './owner-policy-presentation';
import { policyAmountInput, policyDraft, type PolicyAmountDraft } from './owner-policy-amounts';

export type OwnerPolicyFormContext = Readonly<OwnerPolicyContext>;

/** Mount with the current options digest as key; a replaced quote retires this draft. */
export function OwnerPolicyForm({
  context,
  initial,
  active,
  busy,
  onPrepare,
}: {
  context: OwnerPolicyFormContext;
  initial: PolicyAmountDraft;
  active: boolean;
  busy: boolean;
  onPrepare: (draft: PolicyAmountDraft) => void;
}) {
  const { colors, scheme } = useTheme();
  const [limit, setLimit] = useState(() =>
    policyAmountInput(initial.limit_units, context.scale, locale),
  );
  const [attempt, setAttempt] = useState(() =>
    policyAmountInput(initial.max_attempt_units, context.scale, locale),
  );
  const [enabled, setEnabled] = useState(initial.enabled);
  const authenticated = validContext(context);
  // Off reviews the authenticated policy, keeping any unsubmitted text for On.
  // Only an explicit, validated absence has zero limits; malformed data stays blocked.
  const disabledLimit = authenticated ? (context.current_limit_units ?? '0') : '';
  const disabledAttempt = authenticated ? (context.current_max_attempt_units ?? '0') : '';
  const displayedLimit = enabled ? limit : policyAmountInput(disabledLimit, context.scale, locale);
  const displayedAttempt = enabled
    ? attempt
    : policyAmountInput(disabledAttempt, context.scale, locale);
  const value = policyDraft(
    displayedLimit,
    displayedAttempt,
    enabled,
    context.scale,
    context.reservation_units,
    locale,
  );
  const hold = policyAmount(
    context.reservation_units,
    context.currency,
    context.scale,
    formatLocale,
  );
  const available = active && !busy && !!hold && authenticated;
  const showError = enabled && limit.trim() && attempt.trim() && value.error;
  return (
    <Surface>
      <Text selectable style={{ color: colors.secondary, lineHeight: 22 }}>
        {t('ownerPolicy.ownerScope')}
      </Text>
      <Field
        label={t('ownerPolicy.amountField', {
          field: t('ownerPolicy.monthlyLimit'),
          currency: context.currency,
        })}
        value={displayedLimit}
        onChangeText={setLimit}
        editable={available && enabled}
        keyboardType="decimal-pad"
        autoCorrect={false}
      />
      <Field
        label={t('ownerPolicy.amountField', {
          field: t('ownerPolicy.perAttempt'),
          currency: context.currency,
        })}
        value={displayedAttempt}
        onChangeText={setAttempt}
        editable={available && enabled}
        keyboardType="decimal-pad"
        autoCorrect={false}
      />
      <Host matchContents colorScheme={scheme} seedColor={colors.accent}>
        <Switch
          label={t('ownerPolicy.spendingEnabled')}
          value={enabled}
          onValueChange={setEnabled}
          disabled={!available}
        />
      </Host>
      {hold ? (
        <Text selectable style={{ color: colors.secondary, lineHeight: 22 }}>
          {t('ownerPolicy.reservation', { amount: hold, currency: context.currency })}
        </Text>
      ) : null}
      {showError ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger }}>
          {value.error === 'attempt'
            ? t('ownerPolicy.attemptExceedsLimit')
            : value.error === 'reservation'
              ? t('ownerPolicy.belowReservation')
              : t('ownerPolicy.invalidAmount')}
        </Text>
      ) : null}
      <NativeButton
        label={t('ownerPolicy.review')}
        disabled={!available || !value.draft}
        onPress={() => {
          if (available && value.draft) onPrepare(value.draft);
        }}
      />
    </Surface>
  );
}
