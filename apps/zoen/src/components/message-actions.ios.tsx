import { useEffect, useRef, useState, type ComponentProps } from 'react';
import { Modal, Pressable, ScrollView, Text, View, useWindowDimensions } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import Animated, {
  Easing,
  useAnimatedStyle,
  useSharedValue,
  withTiming,
} from 'react-native-reanimated';
import { scheduleOnRN } from 'react-native-worklets';
import { Host } from '@expo/ui';
import { Button, VStack } from '@expo/ui/swift-ui';
import {
  buttonStyle,
  foregroundStyle,
  frame,
  disabled as nativeDisabled,
} from '@expo/ui/swift-ui/modifiers';
import { GlassView, isLiquidGlassAvailable } from 'expo-glass-effect';
import { t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { ReactionPalette, ReactionButton, MORE_REACTIONS } from './reaction-palette';
import type { MessageActionsProps } from './message-actions.types';
import { ExtractedMessageActions } from './message-context-actions.ios';

/** Contextual native modal. Focus/navigation callbacks wait for UIKit's real dismissal. */
export function MessageActions(props: MessageActionsProps) {
  return props.entry && props.nativeSource ? (
    <ExtractedMessageActions
      props={props}
      fallback={(value) => <SafeMessageActions {...value} />}
    />
  ) : (
    <SafeMessageActions {...props} />
  );
}
function SafeMessageActions(props: MessageActionsProps) {
  const { colors, scheme, reducedMotion } = useTheme();
  const window = useWindowDimensions();
  const insets = useSafeAreaInsets();
  const [presentation, setPresentation] = useState<MessageActionsProps | null>(null);
  const [visible, setVisible] = useState(false);
  const [presentedId, setPresentedId] = useState<string | null>(null);
  const [more, setMore] = useState(false);
  const [measurement, setMeasurement] = useState({ key: '', height: 0 });
  const closing = useRef(false);
  const action = useRef<(() => void) | null>(null);
  const mounted = useRef(true);
  const opacity = useSharedValue(0);
  const scale = useSharedValue(0.96);
  const incomingId = props.entry?.id ?? null;
  if (incomingId !== presentedId || (incomingId && props.mode !== presentation?.mode)) {
    setPresentedId(incomingId);
    if (props.entry) {
      setPresentation(props);
      setVisible(true);
      setMore(false);
    } else setVisible(false);
  }
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      action.current = null;
    };
  }, []);
  useEffect(() => {
    closing.current = false;
    action.current = null;
  }, [incomingId, props.mode]);
  const current = props.entry?.id === presentation?.entry?.id ? props : presentation;
  const entry = current?.entry;
  type ActionRow = {
    label: string;
    image: ComponentProps<typeof Button>['systemImage'];
    run?: (entry: NonNullable<MessageActionsProps['entry']>) => void;
    destructive?: boolean;
    disabled?: boolean;
    keepOpen?: boolean;
  };
  const rawRows: ActionRow[] = entry
    ? [
        { label: t('chat.reply'), image: 'arrowshape.turn.up.left', run: current?.onReply },
        {
          label: t('chat.openReplies'),
          image: 'bubble.left.and.bubble.right',
          run: current?.onThread,
        },
        {
          label: t('chat.forward'),
          image: 'arrowshape.turn.up.right',
          run: !entry.deleted ? current?.onForward : undefined,
        },
        { label: t('messageInfo.title'), image: 'info.circle', run: current?.onInfo },
        { label: t('chat.selectMessages'), image: 'checkmark.circle', run: current?.onSelect },
        { label: t('chat.copyText'), image: 'doc.on.doc', run: current?.onCopy },
        { label: t('chat.copyMessageLink'), image: 'link', run: current?.onCopyLink },
        { label: t('media.share'), image: 'square.and.arrow.up', run: current?.onShare },
        {
          label: t('media.prepareNewSend'),
          image: 'arrow.clockwise',
          run:
            entry.author.is_me && !entry.deleted && entry.delivery === 'Failed'
              ? current?.onRecover
              : undefined,
        },
        {
          label: t('chat.edit'),
          image: 'pencil',
          run: entry.author.is_me && !entry.deleted ? current?.onEdit : undefined,
        },
        {
          label: t('chat.deleteForEveryone'),
          image: 'trash',
          run: entry.author.is_me && !entry.deleted ? current?.onDelete : undefined,
          destructive: true,
        },
      ]
    : [];
  const rows: readonly ActionRow[] = current?.options ?? rawRows.filter((row) => row.run);
  const measurementKey = `${entry?.id ?? ''}:${current?.mode}:${more}:${Boolean(current?.error)}:${Boolean(current?.preview)}`;
  const estimatedHeight =
    (current?.options ? 0 : 56) +
    (current?.mode === 'actions' ? rows.length * 48 + 10 : 0) +
    (more ? 238 : 0) +
    (current?.error ? 64 : 0) +
    (current?.preview ? 170 : 0);
  const height = measurement.key === measurementKey ? measurement.height : estimatedHeight;
  const width = Math.min(336, window.width - 32);
  const anchor = current?.anchor;
  const desiredTop = anchor
    ? anchor.y > window.height / 2
      ? anchor.y - height - 10
      : anchor.y + Math.min(anchor.height, 120) + 10
    : window.height * 0.32;
  const top = Math.max(
    insets.top + 12,
    Math.min(desiredTop, window.height - insets.bottom - Math.max(height, 56) - 12),
  );
  const left = Math.max(
    16,
    Math.min(
      anchor
        ? entry?.author.is_me
          ? anchor.x + anchor.width - width
          : anchor.x
        : (window.width - width) / 2,
      window.width - width - 16,
    ),
  );
  const animated = useAnimatedStyle(() => ({
    transform: [{ scale: reducedMotion ? 1 : scale.get() }],
  }));
  // UIVisualEffectView needs alpha 1 throughout its ancestor chain.
  // Fade the separate backdrop; the glass panel only scales.
  const backdrop = useAnimatedStyle(() => ({ opacity: opacity.get() }));
  function closeNative() {
    if (mounted.current) setVisible(false);
  }
  function close(callback?: () => void) {
    if (closing.current || !visible) return;
    closing.current = true;
    action.current = callback ?? null;
    scale.set(withTiming(reducedMotion ? 1 : 0.96, { duration: 100 }));
    opacity.set(
      withTiming(0, { duration: reducedMotion ? 80 : 100 }, (finished) => {
        if (finished) scheduleOnRN(closeNative);
      }),
    );
  }
  const menu = (
    <Host style={{ height: rows.length * 48, width: width - 24 }} colorScheme={scheme}>
      <VStack spacing={0}>
        {rows.map((row) => (
          <Button
            key={row.label}
            label={row.label}
            systemImage={row.image}
            role={row.destructive ? 'destructive' : undefined}
            modifiers={[
              buttonStyle('plain'),
              foregroundStyle(row.destructive ? colors.danger : colors.text),
              frame({ minHeight: 48, maxWidth: Infinity, alignment: 'leading' }),
              nativeDisabled(Boolean(current?.pending || row.disabled)),
            ]}
            onPress={() => {
              if (entry && !current?.pending && !row.disabled) {
                if (row.keepOpen) row.run?.(entry);
                else close(() => row.run?.(entry));
              }
            }}
          />
        ))}
      </VStack>
    </Host>
  );
  return (
    <Modal
      transparent
      visible={visible}
      animationType="none"
      onRequestClose={() => close()}
      onShow={() => {
        opacity.set(
          withTiming(1, {
            duration: reducedMotion ? 80 : 160,
            easing: Easing.bezier(0.23, 1, 0.32, 1),
          }),
        );
        scale.set(
          withTiming(1, {
            duration: reducedMotion ? 0 : 160,
            easing: Easing.bezier(0.23, 1, 0.32, 1),
          }),
        );
      }}
      onDismiss={() => {
        if (!mounted.current) return;
        const callback = action.current;
        action.current = null;
        setPresentation(null);
        closing.current = false;
        props.onClose();
        callback?.();
      }}
    >
      <View accessibilityViewIsModal style={{ flex: 1 }}>
        <Animated.View style={[{ position: 'absolute', inset: 0 }, backdrop]}>
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={t('chat.closeMessageActions')}
            onPress={() => close()}
            style={{ flex: 1, backgroundColor: 'rgba(0,0,0,0.28)' }}
          />
        </Animated.View>
        <Animated.View
          onLayout={({ nativeEvent }) => {
            const height = nativeEvent.layout.height;
            setMeasurement((previous) =>
              previous.key === measurementKey && previous.height === height
                ? previous
                : { key: measurementKey, height },
            );
          }}
          style={[
            {
              position: 'absolute',
              top,
              left,
              width,
              maxHeight: window.height - insets.top - insets.bottom - 24,
              gap: 10,
              transformOrigin: entry?.author.is_me ? 'right top' : 'left top',
            },
            animated,
          ]}
        >
          {entry && !current?.options ? (
            <ReactionPalette
              reactions={entry.reactions}
              pending={current?.pending}
              onReact={(emoji) => {
                if (!current?.pending) current?.onReact(emoji);
              }}
              onMore={() => {
                if (current?.onMore) close(current.onMore);
                else setMore((value) => !value);
              }}
            />
          ) : null}
          {entry && more ? (
            <View style={{ backgroundColor: colors.surface, borderRadius: 20, padding: 8 }}>
              <ScrollView
                bounces={false}
                style={{ maxHeight: 220 }}
                contentContainerStyle={{
                  flexDirection: 'row',
                  flexWrap: 'wrap',
                  justifyContent: 'center',
                }}
              >
                {MORE_REACTIONS.map((emoji) => (
                  <ReactionButton
                    key={emoji}
                    emoji={emoji}
                    mine={entry.reactions.some(
                      (reaction) => reaction.mine && reaction.emoji === emoji,
                    )}
                    pending={Boolean(current?.pending)}
                    onReact={(value) => current?.onReact(value)}
                  />
                ))}
              </ScrollView>
            </View>
          ) : null}
          {current?.error ? (
            <Text
              accessibilityRole="alert"
              style={{
                color: colors.danger,
                backgroundColor: colors.surface,
                borderRadius: 12,
                padding: 12,
              }}
            >
              {current.error}
            </Text>
          ) : null}
          {current?.preview ? (
            current.options ? (
              <ScrollView
                style={{
                  maxHeight: Math.max(
                    120,
                    window.height - insets.top - insets.bottom - rows.length * 48 - 120,
                  ),
                  flexShrink: 1,
                }}
              >
                {current.preview}
              </ScrollView>
            ) : (
              <View style={{ maxHeight: 160, overflow: 'hidden' }}>{current.preview}</View>
            )
          ) : null}
          {current?.mode === 'actions' ? (
            <ScrollView
              bounces={false}
              keyboardShouldPersistTaps="handled"
              style={{ height: rows.length * 48, flexGrow: 0, flexShrink: 1, borderRadius: 20 }}
            >
              {process.env.EXPO_OS === 'ios' && isLiquidGlassAvailable() ? (
                <GlassView
                  colorScheme={scheme}
                  glassEffectStyle="regular"
                  style={{ borderRadius: 20, paddingHorizontal: 12 }}
                >
                  {menu}
                </GlassView>
              ) : (
                <View
                  style={{
                    borderRadius: 20,
                    paddingHorizontal: 12,
                    backgroundColor: colors.raised,
                  }}
                >
                  {menu}
                </View>
              )}
            </ScrollView>
          ) : null}
        </Animated.View>
      </View>
    </Modal>
  );
}
