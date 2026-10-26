import { forwardRef, useCallback, useRef, useState } from 'react';
import { Stack } from 'expo-router';
import {
  ScrollView,
  Text,
  View,
  useWindowDimensions,
  type ScrollViewProps,
  type ScrollViewInstance,
} from 'react-native';
import { useTheme } from '../theme';
import { NavigationContainer } from './navigation-container';
import { NavigationGlass } from './navigation-glass';
import { navigationGlassAppearance } from './navigation-appearance';

type Props = ScrollViewProps & { title: string };

/** The conversation's native scroll-edge material, bound to this screen's own scroll view. */
export const NavigationScrollView = forwardRef<ScrollViewInstance, Props>(
  function NavigationScrollView({ title, children, onLayout, style, ...props }, forwardedRef) {
    const scroll = useRef<ScrollViewInstance | null>(null);
    const [scrollViewTag, setScrollViewTag] = useState(0);
    const { width } = useWindowDimensions();
    const { presentation: p } = useTheme();
    const nativeWidth = Math.max(88, width - 176);
    const bind = useCallback(
      (node: ScrollViewInstance | null) => {
        scroll.current = node;
        if (!node) setScrollViewTag(0);
        if (typeof forwardedRef === 'function') forwardedRef(node);
        else if (forwardedRef) forwardedRef.current = node;
      },
      [forwardedRef],
    );
    return (
      <ScrollView
        {...props}
        ref={bind}
        style={[{ flex: 1, backgroundColor: 'transparent' }, style]}
        contentInsetAdjustmentBehavior="automatic"
        onLayout={(event) => {
          if (process.env.EXPO_OS === 'ios') {
            const tag = scroll.current?.getScrollableNode();
            setScrollViewTag(
              typeof tag === 'number' && Number.isSafeInteger(tag) && tag > 0 ? tag : 0,
            );
          }
          onLayout?.(event);
        }}
      >
        <Stack.Screen
          options={{
            ...navigationGlassAppearance,
            title,
            headerBackButtonDisplayMode: 'minimal',
            headerTintColor: p.navigation.control,
            headerTitleStyle: { ...p.typography.title, color: p.navigation.text },
            ...(process.env.EXPO_OS === 'ios'
              ? {
                  headerBackground: undefined,
                  headerTitle: () => (
                    <NavigationContainer scrollViewTag={scrollViewTag}>
                      <NavigationGlass radius={22}>
                        <View
                          style={{
                            minHeight: 44,
                            minWidth: Math.min(178, nativeWidth),
                            maxWidth: nativeWidth,
                            paddingHorizontal: 14,
                            alignItems: 'center',
                            justifyContent: 'center',
                          }}
                        >
                          <Text
                            accessibilityRole="header"
                            numberOfLines={1}
                            style={{
                              ...p.typography.title,
                              color: p.navigation.text,
                              textAlign: 'center',
                            }}
                          >
                            {title}
                          </Text>
                        </View>
                      </NavigationGlass>
                    </NavigationContainer>
                  ),
                }
              : {}),
          }}
        />
        {children}
      </ScrollView>
    );
  },
);
