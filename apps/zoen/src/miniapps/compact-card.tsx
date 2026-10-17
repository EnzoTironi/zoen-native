import { Image } from 'expo-image';
import { router } from 'expo-router';
import type { ReactNode } from 'react';
import { Pressable, Text, View, useWindowDimensions } from 'react-native';
import { t, formatLocale } from '../i18n/index.ts';
import type { ItemDetail } from '../core/generated/ItemDetail';
import { miniappSnapshot } from '../native-surfaces/snapshot';
import { Icon } from '../components/primitives';
import type { SFSymbol } from 'expo-symbols';

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
};

export type CompactMiniappCardProps = {
  item: ItemDetail;
  size?: number;
  onPress?: () => void;
  onLongPress?: () => void;
  /** A sibling control, so menus never become an interactive child of the card. */
  accessory?: ReactNode;
};

/** In-app content only. No widget consent, publication, commands, or web runtime. */
export function CompactMiniappCard({
  item,
  size = 152,
  onPress,
  onLongPress,
  accessory,
}: CompactMiniappCardProps) {
  const { fontScale } = useWindowDimensions();
  if (!item.app) return null;
  const snapshot = miniappSnapshot(item);
  const side = Math.max(136, size);
  const art = snapshot?.art as keyof typeof artwork | undefined;
  const photo = snapshot?.photo as keyof typeof photos | undefined;
  const photoSource = photo ? photos[photo] : undefined;
  const dark = !!photoSource || art === 'globe';
  const ink = dark ? '#FFFFFF' : '#202733';
  const secondary = dark ? '#E1EAF0' : '#465263';
  const title = snapshot?.title ?? item.title;
  const detail = snapshot?.detail ?? (!snapshot ? item.app?.headline : undefined);
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
    ...(snapshot?.bars.map((bar) => `${bar.label} ${Math.round(bar.value * 100)}%`) ?? []),
    ...(snapshot?.rows.map((row) => row.text) ?? []),
    ...(snapshot?.codes ?? []),
    ...(snapshot?.places ?? []),
    ...(snapshot?.times ?? []),
  ]
    .filter(Boolean)
    .join(', ');
  return (
    <View style={{ width: side, minHeight: side }}>
      <Pressable
        testID={`miniapp-card-${item.id}`}
        accessibilityRole="button"
        accessibilityLabel={summary}
        accessibilityHint={t('miniapps.openMiniapp')}
        onPress={
          onPress ?? (() => router.push({ pathname: '/miniapp/[id]', params: { id: item.id } }))
        }
        onLongPress={onLongPress}
        style={({ pressed }) => ({
          minHeight: side,
          borderRadius: 24,
          borderCurve: 'continuous',
          overflow: 'hidden',
          padding: 12,
          gap: 6,
          backgroundColor: art ? (papers[art] ?? '#F5F4EF') : '#F5F4EF',
          opacity: pressed ? 0.75 : 1,
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
              style={{
                position: 'absolute',
                inset: 0,
                experimental_backgroundImage:
                  'linear-gradient(to bottom, rgba(0,0,0,0.05) 0%, rgba(0,0,0,0.72) 100%)',
              }}
            />
          </>
        ) : null}
        <View style={{ paddingRight: accessory ? 44 : 0, gap: 6 }}>
          {snapshot?.eyebrow ? (
            <Text numberOfLines={1} style={{ color: secondary, fontSize: 11, fontWeight: '600' }}>
              {snapshot.eyebrow}
            </Text>
          ) : null}
          {snapshot?.template === 'ticket' ? (
            <>
              <View style={{ flexDirection: 'row', justifyContent: 'space-between', gap: 8 }}>
                {snapshot.codes.map((code, index) => (
                  <Text
                    key={index}
                    numberOfLines={1}
                    adjustsFontSizeToFit
                    minimumFontScale={0.7}
                    style={{ flex: 1, minWidth: 0, color: ink, fontSize: 24, fontWeight: '700' }}
                  >
                    {code}
                  </Text>
                ))}
              </View>
              <View style={{ flexDirection: 'row', justifyContent: 'space-between', gap: 8 }}>
                {snapshot.times.map((time, index) => (
                  <Text key={index} style={{ color: secondary, fontSize: 12 }}>
                    {time}
                  </Text>
                ))}
              </View>
            </>
          ) : (
            <View
              style={{
                minHeight: photoSource ? side * 0.42 : 48,
                flexDirection: accessory && value ? 'column' : 'row',
                alignItems: accessory && value ? 'flex-start' : 'center',
                gap: 3,
              }}
            >
              {value ? (
                <Text
                  numberOfLines={2}
                  style={{
                    flex: accessory ? undefined : 1,
                    color: ink,
                    fontSize: date ? 25 : 32,
                    fontWeight: '700',
                    fontVariant: ['tabular-nums'],
                  }}
                >
                  {value}
                </Text>
              ) : null}
              {!photoSource && art && artwork[art] ? (
                <Image
                  source={artwork[art]}
                  accessible={false}
                  contentFit="contain"
                  style={{
                    width: value ? (accessory ? 38 : 54) : side - 36 - (accessory ? 44 : 0),
                    height: value ? (accessory ? 38 : 54) : 52,
                  }}
                />
              ) : !photoSource ? (
                <Icon
                  name={{
                    ios: (snapshot?.symbol ?? 'square.grid.2x2') as SFSymbol,
                    android: 'widgets',
                    web: 'widgets',
                  }}
                  size={34}
                  color={ink}
                />
              ) : null}
            </View>
          )}
        </View>
        <Text
          numberOfLines={fontScale > 1.3 ? 3 : 2}
          style={{ color: ink, fontSize: 14, lineHeight: 17, fontWeight: '600' }}
        >
          {title}
        </Text>
        {snapshot?.bars.length ? (
          <View style={{ flexDirection: 'row', gap: 6 }}>
            {snapshot.bars.slice(0, 3).map((bar) => (
              <View key={bar.label} style={{ flex: 1, minWidth: 0, gap: 3 }}>
                <Text numberOfLines={1} style={{ color: secondary, fontSize: 10 }}>
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
                      backgroundColor: snapshot.accentHex,
                    }}
                  />
                </View>
              </View>
            ))}
          </View>
        ) : null}
        {snapshot?.rows.slice(0, 1).map((row, index) => (
          <View key={index} style={{ flexDirection: 'row', gap: 5, alignItems: 'center' }}>
            <Icon
              name={{
                ios: row.done ? 'checkmark.circle.fill' : 'circle',
                android: row.done ? 'check_circle' : 'circle',
                web: row.done ? 'check_circle' : 'circle',
              }}
              size={12}
              color={secondary}
            />
            <Text numberOfLines={1} style={{ flex: 1, fontSize: 11, color: secondary }}>
              {row.text}
            </Text>
          </View>
        ))}
        {detail && !snapshot?.bars.length ? (
          <Text numberOfLines={1} style={{ color: secondary, fontSize: 11, lineHeight: 15 }}>
            {detail}
          </Text>
        ) : null}
      </Pressable>
      {accessory ? (
        <View style={{ position: 'absolute', right: 8, top: 8 }}>{accessory}</View>
      ) : null}
    </View>
  );
}
