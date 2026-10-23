import { useLayoutEffect, useState, type ReactNode } from 'react';
import { Image } from 'expo-image';
import { Animated, StyleSheet, View } from 'react-native';
import { useTheme } from '../theme';

type Sky = ReturnType<typeof useTheme>['sky'];

export function skyGradient(sky: Pick<Sky, 'top' | 'middle' | 'bottom'>) {
  return `linear-gradient(to bottom, ${sky.top} 0%, ${sky.middle} 54%, ${sky.bottom} 100%)`;
}

const photographs = {
  day: require('../../assets/sky/day.jpg'),
  night: require('../../assets/sky/night.jpg'),
};

/** A closely cropped, defocused patch of sky; no illustrated celestial objects. */
export function SkyLayer({ sky }: { sky: Sky }) {
  let cumulativeWeight = 0;
  const layers = (Object.keys(photographs) as (keyof typeof photographs)[])
    .filter((name) => sky.textureWeights[name] > 0)
    .map((name) => {
      cumulativeWeight += sky.textureWeights[name];
      return { name, opacity: sky.textureWeights[name] / cumulativeWeight };
    });
  return (
    <View
      aria-hidden
      style={[
        StyleSheet.absoluteFill,
        {
          backgroundColor: sky.bottom,
          experimental_backgroundImage: skyGradient(sky),
          overflow: 'hidden',
        },
      ]}
    >
      <View style={[StyleSheet.absoluteFill, { opacity: sky.textureOpacity }]}>
        {layers.map(({ name, opacity }) => (
          <Image
            key={name}
            source={photographs[name]}
            accessible={false}
            accessibilityLabel=""
            contentFit="cover"
            contentPosition={{ left: '42%', top: '38%' }}
            blurRadius={20}
            enforceEarlyResizing
            cachePolicy="memory"
            transition={0}
            style={[StyleSheet.absoluteFill, { opacity, transform: [{ scale: 1.8 }] }]}
          />
        ))}
      </View>
      <View
        style={[
          StyleSheet.absoluteFill,
          {
            opacity: sky.haze,
            experimental_backgroundImage:
              'radial-gradient(ellipse at 76% 26%, rgba(255,255,255,0.04) 0%, rgba(255,255,255,0) 72%)',
          },
        ]}
      />
    </View>
  );
}

/** Android tabs share the root photo through their transparent native bar and scenes. */
export function SkyScene({
  children,
  tabScene = false,
}: {
  children: ReactNode;
  tabScene?: boolean;
}) {
  return (
    <View style={{ flex: 1 }}>
      {(!tabScene || process.env.EXPO_OS !== 'android') && <SkyBackground />}
      {children}
    </View>
  );
}

/** An automatic clock sample owns its native value until this one fade unmounts. */
function SkyFade({
  previous,
  current,
  duration,
}: {
  previous: Sky;
  current: Sky;
  duration: number;
}) {
  const [opacity] = useState(() => new Animated.Value(0));
  useLayoutEffect(() => {
    const animation = Animated.timing(opacity, {
      toValue: 1,
      duration,
      useNativeDriver: true,
      isInteraction: false,
    });
    animation.start();
    return () => animation.stop();
  }, [duration, opacity]);
  return (
    <>
      <SkyLayer sky={previous} />
      <Animated.View style={[StyleSheet.absoluteFill, { opacity }]}>
        <SkyLayer sky={current} />
      </Animated.View>
    </>
  );
}

/** A decorative sky shared by navigation, account access, and every transparent scene. */
export function SkyBackground() {
  const { sky, scheme, preference, reducedMotion } = useTheme();
  const { top, middle, bottom, stars: starOpacity, haze, transitionMs } = sky;
  const signature = [
    top,
    middle,
    bottom,
    starOpacity,
    haze,
    transitionMs,
    scheme,
    sky.textureOpacity,
    JSON.stringify(sky.textureWeights),
  ].join(':');
  const [layers, setLayers] = useState({
    signature,
    previous: sky,
    current: sky,
    scheme,
    previousScheme: scheme,
  });
  if (layers.signature !== signature)
    setLayers({
      signature,
      previous: layers.current,
      current: sky,
      scheme,
      previousScheme: layers.scheme,
    });
  // A snap has no outgoing layer or native animated property to retain. An
  // interrupted clock fade cannot lower the opacity of a later selection.
  const snap =
    reducedMotion ||
    preference !== 'auto' ||
    transitionMs <= 1000 ||
    layers.previousScheme !== scheme;
  return (
    <View
      pointerEvents="none"
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={StyleSheet.absoluteFill}
    >
      {snap ? (
        <SkyLayer sky={sky} />
      ) : (
        <SkyFade key={signature} previous={layers.previous} current={sky} duration={transitionMs} />
      )}
    </View>
  );
}
