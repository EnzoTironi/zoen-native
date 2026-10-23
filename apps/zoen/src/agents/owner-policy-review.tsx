import { Text, View } from 'react-native';
import { NativeButton, Surface } from '../components/primitives';
import { t, formatLocale } from '../i18n/index.ts';
import { useTheme } from '../theme';
import {
  policyPresentation,
  type OwnerPolicyReviewFields,
  type PolicyProfileLabel,
} from './owner-policy-presentation';

export type OwnerPolicyReviewPhase =
  | 'ready'
  | 'pending'
  | 'installed'
  | 'refused'
  | 'delivery_failed';

/** A confirmed submission stays pending until the actual Runtime receipt arrives. */
export function OwnerPolicyReview({
  review,
  profiles,
  phase,
  active,
  busy,
  onConfirm,
}: {
  review: OwnerPolicyReviewFields;
  profiles: readonly PolicyProfileLabel[];
  phase: OwnerPolicyReviewPhase;
  active: boolean;
  busy: boolean;
  onConfirm: () => void;
}) {
  const { colors } = useTheme();
  const value = policyPresentation(review, profiles, formatLocale);
  const canConfirm = !!value && active && !busy && phase === 'ready';
  const status =
    phase === 'pending'
      ? t('ownerPolicy.pending')
      : phase === 'installed'
        ? t('ownerPolicy.installed')
        : phase === 'refused'
          ? t('ownerPolicy.refused')
          : phase === 'delivery_failed'
            ? t('ownerPolicy.deliveryFailed')
            : null;
  return (
    <Surface>
      <Text
        accessibilityRole="header"
        style={{ color: colors.text, fontSize: 23, fontWeight: '600' }}
      >
        {t('ownerPolicy.title')}
      </Text>
      <Text selectable style={{ color: colors.secondary, lineHeight: 22 }}>
        {t('ownerPolicy.ownerScope')}
      </Text>
      {value ? (
        <>
          <Text selectable style={{ color: colors.text, fontSize: 17 }}>
            {value.period} · {review.currency}
          </Text>
          {[
            [t('ownerPolicy.monthlyLimit'), value.limit],
            [t('ownerPolicy.perAttempt'), value.attempt],
            [t('ownerPolicy.services'), value.profiles.join(', ')],
          ].map(([label, amount]) => (
            <View key={label} style={{ gap: 6 }}>
              <Text style={{ color: colors.secondary, fontSize: 14 }}>{label}</Text>
              <Text
                selectable
                style={{
                  color: colors.text,
                  fontSize: 20,
                  fontWeight: '600',
                  fontVariant: ['tabular-nums'],
                }}
              >
                {amount}
              </Text>
            </View>
          ))}
          {!review.enabled ? (
            <Text selectable style={{ color: colors.danger }}>
              {t('ownerPolicy.disabled')}
            </Text>
          ) : null}
          <Text selectable style={{ color: colors.secondary }}>
            {t('ownerPolicy.expires', { date: value.expires })}
          </Text>
        </>
      ) : (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger }}>
          {t('ownerPolicy.unavailable')}
        </Text>
      )}
      {status ? (
        <Text
          accessibilityLiveRegion="polite"
          selectable
          style={{
            color:
              phase === 'refused' || phase === 'delivery_failed' ? colors.danger : colors.secondary,
          }}
        >
          {status}
        </Text>
      ) : null}
      {phase === 'ready' ? (
        <NativeButton
          label={review.enabled ? t('ownerPolicy.confirm') : t('ownerPolicy.disable')}
          disabled={!canConfirm}
          onPress={() => {
            if (canConfirm) onConfirm();
          }}
        />
      ) : null}
    </Surface>
  );
}
