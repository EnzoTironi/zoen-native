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
  warm: require('../../assets/sky/warm.jpg'),
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

/** Native tab controllers own an opaque scene; paint the photo inside it. */
export function SkyScene({ children }: { children: ReactNode }) {
  return (
    <View style={{ flex: 1 }}>
      <SkyBackground />
      {children}
    </View>
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
  const [opacity] = useState(() => new Animated.Value(1));
  if (layers.signature !== signature)
    setLayers({
      signature,
      previous: layers.current,
      current: sky,
      scheme,
      previousScheme: layers.scheme,
    });
  useLayoutEffect(() => {
    // Manual/resume selections change their semantic inks synchronously. Only
    // tiny clock-driven samples approved by the theme's contrast guard fade.
    // No old bright sky beneath new light ink, including rapid manual changes.
    const snap =
      reducedMotion ||
      preference !== 'auto' ||
      transitionMs <= 1000 ||
      layers.previousScheme !== scheme;
    opacity.setValue(snap ? 1 : 0);
    if (snap) return;
    const animation = Animated.timing(opacity, {
      toValue: 1,
      duration: transitionMs,
      useNativeDriver: true,
      isInteraction: false,
    });
    animation.start();
    return () => animation.stop();
  }, [
    top,
    middle,
    bottom,
    starOpacity,
    haze,
    transitionMs,
    scheme,
    preference,
    layers.previousScheme,
    signature,
    reducedMotion,
    opacity,
  ]);
  return (
    <View
      pointerEvents="none"
      accessibilityElementsHidden
      importantForAccessibility="no-hide-descendants"
      style={StyleSheet.absoluteFill}
    >
      <SkyLayer sky={layers.previous} />
      <Animated.View style={[StyleSheet.absoluteFill, { opacity }]}>
        <SkyLayer sky={layers.current} />
      </Animated.View>
    </View>
  );
}
