import { Image } from 'expo-image';
import { PHOTO_CARD_TEXT, PHOTO_CARD_SECONDARY, PHOTO_CARD_TEXT_GRADIENT } from './photo-card';
import { router } from 'expo-router';
import type { ReactNode } from 'react';
import { Text, View, useWindowDimensions, type AccessibilityActionEvent } from 'react-native';
import { MessagePressable as Pressable } from '../components/message-interaction';
import { t, formatLocale } from '../i18n/index.ts';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { useTheme } from '../theme';
import { miniappSnapshot } from '../native-surfaces/snapshot';
import { Icon } from '../components/primitives';
import type { SFSymbol } from 'expo-symbols';
import { APP_CARD_METRICS as M, appCardContentLayout } from './app-card-layout';

const artwork = {
  pet: require('../../assets/miniapps/pet.svg'),
  'pet.asleep': require('../../assets/miniapps/pet-asleep.svg'),
  'pet.gone': require('../../assets/miniapps/pet-gone.svg'),
  globe: require('../../assets/miniapps/globe.svg'),
  pot: require('../../assets/miniapps/pot.svg'),
  ballot: require('../../assets/miniapps/ballot.svg'),
  notepad: require('../../assets/miniapps/notepad.svg'),
  trip: require('../../assets/miniapps/trip.svg'),
  hike: require('../../assets/miniapps/hike.svg'),
  drawing: require('../../assets/miniapps/drawing.svg'),
  slides: require('../../assets/miniapps/slides.svg'),
};
const photos = {
  'hike-tomales': require('../../assets/miniapps/hike-tomales.jpg'),
  'hike-steep': require('../../assets/miniapps/hike-steep.jpg'),
  'hike-lands': require('../../assets/miniapps/hike-lands.jpg'),
};
const papers: Record<string, string> = {
  pet: '#E8EEDC',
  'pet.asleep': '#DDE5F0',
  'pet.gone': '#E8EEDC',
  globe: '#152C43',
  pot: '#F8E0BE',
  ballot: '#E9EEFA',
  notepad: '#FFF9EB',
  trip: '#E1EBEE',
  hike: '#E2EBCF',
  drawing: '#EAF2FB',
  slides: '#F7E9DA',
};

export type MiniappCardItem = Pick<ItemDetail, 'id' | 'version' | 'title' | 'app'> & {
  /** Unknown message scope can show a plain cover, never a trusted snapshot. */
  space_id?: string;
};
export type CompactMiniappCardProps = {
  item: MiniappCardItem;
  onPress?: () => void;
  onAccessibilityTap?: () => void;
  onLongPress?: () => void;
  editing?: boolean;
  accessibilityHint?: string;
  accessibilityActions?: { name: string; label: string }[];
  onAccessibilityAction?: (event: AccessibilityActionEvent) => void;
  /** Sibling 44pt controls; never an interactive descendant of the App action. */
  editActions?: ReactNode;
};

/** Native cover only. No widget consent, publication, commands or embedded runtime. */
export function CompactMiniappCard({
  item,
  onPress,
  onAccessibilityTap,
  onLongPress,
  editing = false,
  accessibilityHint,
  accessibilityActions,
  onAccessibilityAction,
  editActions,
}: CompactMiniappCardProps) {
  const { fontScale } = useWindowDimensions();
  const { reducedMotion } = useTheme();
  if (!item.app) return null;
  const snapshot =
    typeof item.space_id === 'string'
      ? miniappSnapshot({ ...item, space_id: item.space_id })
      : null;
  const layout = appCardContentLayout(fontScale, !!editActions);
  const art = snapshot?.art as keyof typeof artwork | undefined;
  const photo = snapshot?.photo as keyof typeof photos | undefined;
  const photoSource = photo ? photos[photo] : undefined;
  const dark = !!photoSource || art === 'globe';
  const ink = dark ? PHOTO_CARD_TEXT : '#202733';
  const secondary = dark ? PHOTO_CARD_SECONDARY : '#465263';
  const title = snapshot?.title ?? item.title;
  const detail = snapshot?.detail ?? (!snapshot ? item.app.headline : undefined);
  const date =
    snapshot?.template === 'countdown' && snapshot.targetMs !== undefined
      ? new Date(snapshot.targetMs).toLocaleDateString(formatLocale, {
          day: 'numeric',
          month: 'short',
        })
      : undefined;
  const value = snapshot?.value ?? date;
  const summary = [
    title,
    title !== item.title ? item.title : undefined,
    snapshot?.eyebrow,
    value,
    detail,
    item.app.headline !== detail ? item.app.headline : undefined,
    ...(snapshot?.bars.map((bar) => `${bar.label} ${Math.round(bar.value * 100)}%`) ?? []),
    ...(snapshot?.rows.map((row) => row.text) ?? []),
    ...(snapshot?.codes ?? []),
    ...(snapshot?.places ?? []),
    ...(snapshot?.times ?? []),
  ]
    .filter(Boolean)
    .join(', ');
  const bars = snapshot?.bars ?? [];
  const row = snapshot?.rows[0];
  const metadata = detail ?? snapshot?.eyebrow;
  return (
    <View style={{ width: M.size, height: M.size, flexShrink: 0 }}>
      <Pressable
        testID={`miniapp-card-${item.id}`}
        accessibilityRole="button"
        accessibilityLabel={summary}
        accessibilityHint={accessibilityHint ?? t('miniapps.openMiniapp')}
        accessibilityActions={accessibilityActions}
        onAccessibilityAction={onAccessibilityAction}
        onPress={
          onPress ?? (() => router.push({ pathname: '/miniapp/[id]', params: { id: item.id } }))
        }
        onLongPress={onLongPress}
        onAccessibilityTap={onAccessibilityTap}
        style={({ pressed }) => ({
          width: M.size,
          height: M.size,
          borderRadius: M.radius,
          borderCurve: 'continuous',
          overflow: 'hidden',
          backgroundColor: art ? (papers[art] ?? '#F5F4EF') : '#F5F4EF',
          opacity: pressed && !photoSource && !editing ? 0.75 : 1,
          transform: [{ scale: pressed && photoSource && !reducedMotion && !editing ? 0.985 : 1 }],
        })}
      >
        {photoSource ? (
          <>
            <Image
              source={photoSource}
              contentFit="cover"
              accessible={false}
              style={{ position: 'absolute', inset: 0 }}
            />
            <View
              pointerEvents="none"
              style={{
                position: 'absolute',
                inset: 0,
                experimental_backgroundImage: PHOTO_CARD_TEXT_GRADIENT,
              }}
            />
          </>
        ) : null}
        <View
          pointerEvents="none"
          accessible={false}
          accessibilityElementsHidden
          importantForAccessibility="no-hide-descendants"
          style={{ position: 'absolute', inset: 0 }}
        >
          {layout.showVisual ? (
            <View
              testID={`miniapp-card-visual-${item.id}`}
              style={{
                position: 'absolute',
                top: M.padding,
                left: M.padding,
                width: layout.visualWidth,
                height: layout.visualHeight,
                flexDirection: 'row',
                alignItems: 'center',
                gap: M.contentGap,
              }}
            >
              {snapshot?.template === 'ticket' ? (
                <View style={{ flex: 1, gap: 3 }}>
                  <View style={{ flexDirection: 'row', gap: M.contentGap }}>
                    {snapshot.codes.map((code, index) => (
                      <Text
                        key={index}
                        numberOfLines={1}
                        style={{
                          flex: 1,
                          minWidth: 0,
                          color: ink,
                          fontSize: M.codeFontSize,
                          lineHeight: M.codeLineHeight,
                          fontWeight: '700',
                        }}
                      >
                        {code}
                      </Text>
                    ))}
                  </View>
                  {layout.showMetadata ? (
                    <View style={{ flexDirection: 'row', gap: M.contentGap }}>
                      {snapshot.times.map((time, index) => (
                        <Text
                          key={index}
                          numberOfLines={1}
                          style={{
                            flex: 1,
                            minWidth: 0,
                            color: secondary,
                            fontSize: M.detailFontSize,
                            lineHeight: M.detailLineHeight,
                          }}
                        >
                          {time}
                        </Text>
                      ))}
                    </View>
                  ) : null}
                </View>
              ) : (
                <>
                  {value ? (
                    <Text
                      numberOfLines={1}
                      style={{
                        flex: 1,
                        minWidth: 0,
                        color: ink,
                        fontSize: date ? M.dateFontSize : M.valueFontSize,
                        lineHeight: date ? M.dateLineHeight : M.valueLineHeight,
                        fontWeight: '700',
                        fontVariant: ['tabular-nums'],
                      }}
                    >
                      {value}
                    </Text>
                  ) : null}
                  {!photoSource && (!value || !editActions) ? (
                    art && artwork[art] ? (
                      <Image
                        source={artwork[art]}
                        accessible={false}
                        contentFit="contain"
                        style={{ width: value ? 42 : layout.visualWidth, height: M.artworkSize }}
                      />
                    ) : (
                      <Icon
                        name={{
                          ios: (snapshot?.symbol ?? 'square.grid.2x2') as SFSymbol,
                          android: 'widgets',
                          web: 'widgets',
                        }}
                        size={34}
                        color={ink}
                      />
                    )
                  ) : null}
                </>
              )}
            </View>
          ) : null}
          {layout.titleLines > 0 ? (
            <View
              testID={`miniapp-card-title-${item.id}`}
              style={{
                position: 'absolute',
                top: layout.titleTop,
                left: M.padding,
                right: M.padding,
                height: layout.titleHeight,
                justifyContent: 'center',
              }}
            >
              <Text
                numberOfLines={layout.titleLines}
                ellipsizeMode="tail"
                style={{
                  color: ink,
                  fontSize: M.titleFontSize,
                  lineHeight: M.titleLineHeight,
                  fontWeight: '600',
                }}
              >
                {title}
              </Text>
            </View>
          ) : null}
          {layout.showMetadata ? (
            <View
              testID={`miniapp-card-metadata-${item.id}`}
              style={{
                position: 'absolute',
                top: layout.metadataTop,
                left: M.padding,
                right: M.padding,
                height: layout.metadataHeight,
                justifyContent: 'flex-end',
              }}
            >
              {bars.length ? (
                <View style={{ flexDirection: 'row', gap: M.contentGap }}>
                  {bars.slice(0, 3).map((bar) => (
                    <View key={bar.label} style={{ flex: 1, minWidth: 0, gap: 3 }}>
                      <Text
                        numberOfLines={1}
                        style={{ color: secondary, fontSize: 10, lineHeight: 12 }}
                      >
                        {bar.label}
                      </Text>
                      <View
                        style={{
                          height: 4,
                          borderRadius: 2,
                          backgroundColor: '#20273315',
                          overflow: 'hidden',
                        }}
                      >
                        <View
                          style={{
                            width: `${bar.value * 100}%`,
                            height: 4,
                            backgroundColor: snapshot?.accentHex,
                          }}
                        />
                      </View>
                    </View>
                  ))}
                </View>
              ) : row ? (
                <View style={{ flexDirection: 'row', gap: 5, alignItems: 'center' }}>
                  <Icon
                    name={{
                      ios: row.done ? 'checkmark.circle.fill' : 'circle',
                      android: row.done ? 'check_circle' : 'circle',
                      web: row.done ? 'check_circle' : 'circle',
                    }}
                    size={12}
                    color={secondary}
                  />
                  <Text
                    numberOfLines={1}
                    style={{
                      flex: 1,
                      color: secondary,
                      fontSize: M.detailFontSize,
                      lineHeight: M.detailLineHeight,
                    }}
                  >
                    {row.text}
                  </Text>
                </View>
              ) : metadata ? (
                <Text
                  numberOfLines={1}
                  style={{
                    color: secondary,
                    fontSize: M.detailFontSize,
                    lineHeight: M.detailLineHeight,
                  }}
                >
                  {metadata}
                </Text>
              ) : null}
            </View>
          ) : null}
        </View>
      </Pressable>
      {editActions ? (
        <View
          testID={`app-card-edit-actions-${item.id}`}
          style={{
            position: 'absolute',
            left: M.padding,
            right: M.padding,
            top: M.padding,
            height: M.menuSize,
          }}
        >
          {editActions}
        </View>
      ) : null}
    </View>
  );
}
