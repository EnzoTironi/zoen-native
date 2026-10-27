import {
  useCallback,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type Ref,
} from 'react';
import {
  findNodeHandle,
  TextInput,
  StyleSheet,
  useWindowDimensions,
  type TextInputInstance,
  type TextInputProps,
} from 'react-native';
import ZoenCore from '../../modules/zoen-core/src/ZoenCoreModule';
import Animated, { makeMutable, useEvent } from 'react-native-reanimated';
import { createComposerFitting, type ComposerFittingSnapshot } from '../core/composer-fitting';
import { useAttachmentKeyboardField } from '../media/attachment-keyboard-context';

const Input =
  process.env.EXPO_OS === 'ios' ? Animated.createAnimatedComponent(TextInput) : TextInput;
type NativeText = { text: string; eventCount: number };
type FieldProps = TextInputProps & { onSend?: () => void };
function createOwner(context: string) {
  return { context, nativeText: makeMutable<NativeText | null>(null) };
}

/** Keep one editor mounted while its normal, edit, search and caption values change. */
export function ComposerTextField({
  inputRef,
  bindSend,
  context,
  onSend,
  ...props
}: FieldProps & {
  inputRef?: Ref<TextInputInstance>;
  bindSend?: (send: () => void) => void;
  context: string;
}) {
  const field = useRef<TextInputInstance | null>(null);
  const owner = useMemo(() => createOwner(context), [context]);
  const committed = useRef<{ owner: object; props: FieldProps } | null>(null);
  useLayoutEffect(() => {
    const active = { owner, props: { ...props, onSend } };
    committed.current = active;
    return () => {
      if (committed.current === active) committed.current = null;
    };
  });
  const events = () => (committed.current?.owner === owner ? committed.current.props : null);
  // This is an event receipt, never a second draft. The existing writer still
  // owns persistence, Send, acceptance and newer typing.
  const nativeText = owner.nativeText;
  const applied = useRef<{ owner: object; eventCount: number } | null>(null);
  const delivery = useRef<{ owner: object; text: string; accepted: boolean } | null>(null);
  const observeNativeText = useEvent<{ text: string; eventCount: number }>(
    (event) => {
      'worklet';
      const previous = nativeText.get();
      if (previous && previous.eventCount >= event.eventCount) return;
      nativeText.set({ text: event.text, eventCount: event.eventCount });
    },
    ['onChange'],
    true,
  );
  useLayoutEffect(() => {
    // A previously delivered capability remains valid through same-owner
    // rerenders. Its events() fence retires it on handoff or unmount.
    bindSend?.(() => {
      const active = events();
      if (!field.current || !active || active.editable === false) return;
      const captured = process.env.EXPO_OS === 'ios' ? nativeText.get() : null;
      if (
        captured &&
        (applied.current?.owner !== owner || captured.eventCount > applied.current.eventCount)
      ) {
        applied.current = { owner, eventCount: captured.eventCount };
        active.onChangeText?.(captured.text);
      }
      events()?.onSend?.();
    });
  });
  const [width, setWidth] = useState(0);
  const [tag, setTag] = useState<number | null>(null);
  useAttachmentKeyboardField(tag, context, props.accessibilityLabel ?? '', props.placeholder ?? '');
  const { fontScale } = useWindowDimensions();
  const styledFont = StyleSheet.flatten(props.style)?.fontSize;
  const fontSize =
    typeof styledFont === 'number' && Number.isFinite(styledFont) && styledFont > 0
      ? styledFont
      : process.env.EXPO_OS === 'ios'
        ? 17
        : 16;
  const scale =
    props.allowFontScaling === false
      ? 1
      : props.maxFontSizeMultiplier && props.maxFontSizeMultiplier > 0
        ? Math.min(fontScale, props.maxFontSizeMultiplier)
        : fontScale;
  const [fitted, setFitted] = useState<{
    snapshot: ComposerFittingSnapshot;
    height: number;
  } | null>(null);
  const [fitting] = useState(() =>
    createComposerFitting(
      async ({ tag, text, label, placeholder, width, fontSize, nonce }) =>
        ZoenCore?.measureComposer?.(tag, text, label, placeholder, width, fontSize, nonce) ?? null,
      (height, snapshot) => setFitted({ snapshot, height }),
      (work) => {
        const frame = requestAnimationFrame(work);
        return () => cancelAnimationFrame(frame);
      },
    ),
  );
  const snapshot = useMemo(
    () => ({
      tag,
      text: props.value ?? '',
      label: props.accessibilityLabel ?? '',
      placeholder: props.placeholder ?? '',
      width,
      fontSize: fontSize * scale,
      context: `${context}:${props.placeholder ?? ''}`,
    }),
    [
      tag,
      props.value,
      props.accessibilityLabel,
      props.placeholder,
      width,
      fontSize,
      scale,
      context,
    ],
  );
  const current = useRef<ComposerFittingSnapshot | null>(null);
  useLayoutEffect(() => {
    current.current = snapshot;
    if (process.env.EXPO_OS === 'ios' && ZoenCore?.measureComposer) fitting.request(snapshot);
    return () => {
      current.current = null;
      fitting.cancel();
    };
  }, [fitting, snapshot]);
  useImperativeHandle(inputRef, () => field.current!);
  const ref = useCallback(
    (node: TextInputInstance | null) => {
      field.current = node;
      setTag(process.env.EXPO_OS === 'ios' && node ? (findNodeHandle(node) ?? null) : null);
    },
    [setTag],
  );
  // Typing keeps the last proven height until the same editor's new fit lands.
  // Another target, width or font must establish its own geometry.
  const retainedHeight =
    fitted &&
    fitted.snapshot.context === snapshot.context &&
    fitted.snapshot.tag === snapshot.tag &&
    fitted.snapshot.width === snapshot.width &&
    fitted.snapshot.fontSize === snapshot.fontSize
      ? fitted.height
      : undefined;
  return (
    <Input
      // Keep the callable RN listener after the independent UI observer.
      // Reanimated's placeholder must not replace TextInput's _onChange path.
      {...(process.env.EXPO_OS === 'ios' ? { onNativeChange: observeNativeText } : {})}
      {...props}
      ref={ref}
      onChangeText={(text) => {
        const active = events();
        if (
          active?.editable !== false &&
          !(
            delivery.current?.owner === owner &&
            delivery.current.text === text &&
            !delivery.current.accepted
          )
        )
          active?.onChangeText?.(text);
      }}
      onChange={(event) => {
        const active = events();
        if (!active || active.editable === false) return;
        const { text, eventCount } = event.nativeEvent;
        const accepted =
          applied.current?.owner !== owner || eventCount > applied.current.eventCount;
        delivery.current = { owner, text, accepted };
        if (accepted) {
          applied.current = { owner, eventCount };
          active.onChange?.(event);
        }
      }}
      onSelectionChange={(event) => events()?.onSelectionChange?.(event)}
      onFocus={(event) => events()?.onFocus?.(event)}
      onBlur={(event) => events()?.onBlur?.(event)}
      onSubmitEditing={(event) => {
        const active = events();
        if (active?.editable !== false) active?.onSubmitEditing?.(event);
      }}
      onLayout={(event) => {
        const active = events();
        if (!active) return;
        setWidth(event.nativeEvent.layout.width);
        active.onLayout?.(event);
      }}
      onContentSizeChange={(event) => {
        // Content-size events carry no text identity. Remeasure the actual
        // current editor instead of applying a late event's old height.
        if (current.current && process.env.EXPO_OS === 'ios' && ZoenCore?.measureComposer)
          fitting.request(current.current);
        props.onContentSizeChange?.(event);
      }}
      style={[
        props.style,
        {
          height:
            fitted?.snapshot === snapshot
              ? fitted.height
              : snapshot.text.length === 0
                ? process.env.EXPO_OS === 'ios'
                  ? 40
                  : 52
                : retainedHeight,
        },
      ]}
    />
  );
}
