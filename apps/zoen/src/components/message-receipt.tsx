import { View } from 'react-native';
import type { Delivery } from '../core/generated/Delivery';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { contrastRatio } from '../theme/sky.ts';
import { Icon } from './primitives';
import { MessageReceiptGraphic } from './message-graphics';

export function messageDeliveryLabel(delivery: Delivery): string {
  switch (delivery) {
    case 'Local':
      return t('chat.receiptLocal');
    case 'Sending':
      return t('chat.receiptSending');
    case 'Sent':
      return t('chat.receiptSent');
    case 'Delivered':
      return t('chat.receiptDelivered');
    case 'Read':
      return t('chat.receiptRead');
    case 'Failed':
      return t('chat.receiptFailed');
  }
}

/** Receipts reflect the core's acknowledgements, never a presentation timer. */
export function MessageReceipt({
  delivery,
  ownerKey,
  ink,
  background,
}: {
  delivery: Delivery;
  ownerKey?: string;
  ink?: string;
  background?: string;
}) {
  const { colors } = useTheme();
  const doubled = delivery === 'Delivered' || delivery === 'Read';
  const readBlue =
    ['#168CD2', '#4CC3F2', '#1167A3', '#79D7FF'].find(
      (color) => contrastRatio(color, background ?? colors.mine) >= 3,
    ) ??
    ink ??
    colors.onMine;
  const color = delivery === 'Read' ? readBlue : (ink ?? colors.onMine);
  const name =
    delivery === 'Sending'
      ? { ios: 'clock' as const, android: 'schedule' as const, web: 'schedule' as const }
      : delivery === 'Failed'
        ? {
            ios: 'exclamationmark.circle' as const,
            android: 'error_outline' as const,
            web: 'error_outline' as const,
          }
        : delivery === 'Local'
          ? { ios: 'iphone' as const, android: 'smartphone' as const, web: 'smartphone' as const }
          : { ios: 'checkmark' as const, android: 'check' as const, web: 'check' as const };
  return (
    <View
      accessible
      accessibilityRole="text"
      accessibilityLabel={messageDeliveryLabel(delivery)}
      testID={`message-receipt-${delivery.toLowerCase()}`}
      style={{ width: 13, height: 14, justifyContent: 'center' }}
    >
      {delivery === 'Local' || delivery === 'Failed' ? (
        <Icon name={name} size={11} color={color} />
      ) : (
        <MessageReceiptGraphic
          ownerKey={ownerKey}
          shape={delivery === 'Sending' ? 'clock' : doubled ? 'double' : 'single'}
          color={color}
        />
      )}
    </View>
  );
}
