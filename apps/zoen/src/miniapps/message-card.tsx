import { router } from 'expo-router';
import { Text, View } from 'react-native';
import { MessagePressable as Pressable } from '../components/message-interaction';
import type { ItemCard } from '../core/generated/ItemCard';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { t, locale } from '../i18n/index.ts';
import { formatMoney } from '../i18n/money.ts';
import { useTheme } from '../theme';
import { Icon } from '../components/primitives';
import { CompactMiniappCard } from './compact-card';
import { miniappMessageProjection } from './message-card-model';

export type MiniappMessageCardProps = {
  card: ItemCard;
  item?: ItemDetail | null;
  /** The real conversation scope, needed when only the embedded card is available. */
  spaceId?: string;
  width: number;
  onLongPress?: () => void;
  onPress?: () => void;
};

/** Message attachment: a single meaningful open action, with the same canonical
 * art and snapshot as Home. No widget permission, hidden mutation or network read. */
export function MiniappMessageCard({
  card,
  item,
  spaceId,
  width,
  onLongPress,
  onPress,
}: MiniappMessageCardProps) {
  const { colors, reducedMotion } = useTheme();
  if (card.kind_id === 'file') return null;
  const projection = miniappMessageProjection(card, item, spaceId);
  const side = Math.max(136, Number.isFinite(width) ? width : 240);
  const open =
    onPress ??
    (() =>
      router.push({
        pathname: card.app ? '/miniapp/[id]' : '/item/[id]',
        params: { id: card.item_id },
      }));
  const footer = (
    <View
      pointerEvents="none"
      accessible={false}
      style={{
        flexDirection: 'row',
        alignItems: 'center',
        gap: 10,
        paddingHorizontal: 12,
        paddingVertical: 12,
        minHeight: 44,
        backgroundColor: colors.surface,
        borderRadius: 12,
      }}
    >
      <Icon
        name={{
          ios: card.app ? 'square.grid.2x2' : 'doc.text',
          android: card.app ? 'widgets' : 'description',
          web: card.app ? 'widgets' : 'description',
        }}
        color={colors.secondary}
        size={18}
      />
      <Text
        numberOfLines={2}
        style={{ flex: 1, color: colors.text, fontSize: 14, lineHeight: 19, fontWeight: '600' }}
      >
        {t(card.app ? 'miniapps.openMiniapp' : 'miniapps.openSharedItem')}
      </Text>
      <Icon
        name={{ ios: 'arrow.up.right', android: 'north_east', web: 'north_east' }}
        color={colors.secondary}
        size={15}
      />
    </View>
  );
  if (projection)
    return (
      <View testID={`miniapp-message-${card.item_id}`} style={{ width: side }}>
        <CompactMiniappCard
          item={projection}
          size={side}
          layout="message"
          onPress={open}
          onLongPress={onLongPress}
          footer={footer}
        />
      </View>
    );
  let total: string | null = null;
  if (card.kind_id === 'plan' && card.total_cents !== null) {
    try {
      total = formatMoney(card.total_cents, { locale, currency: 'BRL', minorUnitDigits: 2 });
    } catch {
      /* Invalid money never becomes a guessed or rounded amount. */
    }
  }
  const title = card.title;
  const summary = card.app?.headline || card.summary;
  return (
    <Pressable
      testID={`item-message-${card.item_id}`}
      accessibilityRole="button"
      accessibilityLabel={[title, summary, total].filter(Boolean).join(', ')}
      accessibilityHint={t(card.app ? 'miniapps.openMiniapp' : 'miniapps.openSharedItem')}
      onPress={open}
      onLongPress={onLongPress}
      style={({ pressed }) => ({
        width: side,
        borderRadius: 24,
        borderCurve: 'continuous',
        padding: 14,
        gap: 14,
        backgroundColor: colors.surface,
        opacity: pressed ? 0.8 : 1,
        transform: [{ scale: pressed && !reducedMotion ? 0.985 : 1 }],
      })}
    >
      <View style={{ gap: 8 }}>
        <Text style={{ color: colors.secondary, fontSize: 12, fontWeight: '600' }}>
          {card.kind_label}
        </Text>
        <Text style={{ color: colors.text, fontSize: 22, lineHeight: 28, fontWeight: '600' }}>
          {title}
        </Text>
        {summary ? (
          <Text numberOfLines={4} style={{ color: colors.secondary, fontSize: 15, lineHeight: 22 }}>
            {summary}
          </Text>
        ) : null}
        {total ? (
          <Text
            style={{
              color: colors.text,
              fontSize: 22,
              lineHeight: 28,
              fontWeight: '600',
              fontVariant: ['tabular-nums'],
            }}
          >
            {total}
          </Text>
        ) : null}
        {card.kind_id === 'plan' && card.line_count > 0 ? (
          <Text style={{ color: colors.secondary, fontSize: 13 }}>
            {t('miniapps.messageCardProgress', { done: card.done_count, total: card.line_count })}
          </Text>
        ) : null}
      </View>
      {footer}
    </Pressable>
  );
}
