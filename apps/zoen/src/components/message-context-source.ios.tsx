import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useImperativeHandle,
  useRef,
  useState,
  use,
  type ReactNode,
  type Ref,
} from 'react';
import { AppState, type ViewProps } from 'react-native';
import { useFocusEffect } from 'expo-router';
import { requireNativeView, requireOptionalNativeModule } from 'expo';
import { randomUUID } from 'expo-crypto';
import {
  MessageContextOwnsPress,
  MessageContextStageEnabled,
  MessageContextStageCommands,
  type MessageContextStageHandle,
  type MessageContextGesture,
  type MessageContextNative,
} from './message-context-ports';
import type { MessageContextSourceProps } from './message-context-source';
export { MessageContextControl } from './message-context-control';

const available =
  requireOptionalNativeModule<{ messageContextVersion?: number }>('ZoenCore')
    ?.messageContextVersion === 2;
const NativeStage = available
  ? requireNativeView<ViewProps & { active: boolean; ref?: Ref<MessageContextStageHandle> }>(
      'ZoenCore',
      'MessageContextStage',
    )
  : null;
const NativeSource = available
  ? requireNativeView<
      ViewProps & {
        binding: string;
        active: boolean;
        handlesTap: boolean;
        reducedMotion: boolean;
        ref?: Ref<MessageContextNative>;
        onGesture(event: { nativeEvent: MessageContextGesture }): void;
      }
    >('ZoenCore', 'MessageContextSource')
  : null;

/** One Stage under the original RNGH/Fabric root, outside the navigation subtree. */
export function MessageContextStage({
  children,
  active = true,
  ref,
}: {
  children: ReactNode;
  active?: boolean;
  ref?: Ref<MessageContextStageHandle>;
}) {
  const native = useRef<MessageContextStageHandle | null>(null);
  useImperativeHandle(
    ref,
    () => ({
      retireContexts: async () => {
        await native.current?.retireContexts();
      },
      dismissContexts: async () => {
        await native.current?.dismissContexts();
      },
    }),
    [],
  );
  const dismiss = useCallback(async () => {
    await native.current?.dismissContexts();
  }, []);
  return NativeStage ? (
    <NativeStage ref={native as never} active={active} style={{ flex: 1 }} collapsable={false}>
      <MessageContextStageEnabled value={true}>
        <MessageContextStageCommands value={dismiss}>{children}</MessageContextStageCommands>
      </MessageContextStageEnabled>
    </NativeStage>
  ) : (
    children
  );
}
export function MessageContextSource(props: MessageContextSourceProps) {
  return <BoundMessageContextSource key={props.ownerKey} {...props} />;
}

/** Scope is a React lifetime: never reuse an old native row admission on recycle. */
function BoundMessageContextSource({
  active,
  handlesTap,
  reducedMotion = false,
  children,
  onGesture,
  style,
}: MessageContextSourceProps) {
  const staged = use(MessageContextStageEnabled);
  const ref = useRef<MessageContextNative | null>(null);
  const [binding] = useState(randomUUID);
  const lifetime = useRef({
    binding: '',
    live: false,
    focused: false,
    active: false,
    foreground: AppState.currentState === 'active',
  });
  const latest = useRef(onGesture);
  const received = useRef({ binding, sequence: 0, gestureId: '' });
  useLayoutEffect(() => {
    latest.current = onGesture;
    const state = lifetime.current;
    Object.assign(state, { binding, live: true, active });
    return () => {
      state.live = false;
    };
  }, [binding, active, onGesture]);
  useFocusEffect(
    useCallback(() => {
      const state = lifetime.current;
      state.focused = true;
      return () => {
        state.focused = false;
      };
    }, []),
  );
  useEffect(() => {
    const listener = AppState.addEventListener('change', (state) => {
      lifetime.current.foreground = state === 'active';
    });
    return () => listener.remove();
  }, []);
  if (!NativeSource || !staged) return children;
  return (
    <NativeSource
      ref={ref as never}
      binding={binding}
      active={active}
      handlesTap={handlesTap}
      reducedMotion={reducedMotion}
      collapsable={false}
      style={style}
      onGesture={({ nativeEvent: event }) => {
        const native = ref.current;
        const current = () => {
          const state = lifetime.current;
          return (
            state.live &&
            state.focused &&
            state.active &&
            state.foreground &&
            state.binding === binding &&
            ref.current === native
          );
        };
        if (
          !native ||
          event.binding !== binding ||
          !current() ||
          !['tap', 'hold', 'double'].includes(event.kind) ||
          !Number.isSafeInteger(event.sequence) ||
          event.sequence <= 0
        )
          return;
        if (received.current.binding !== binding)
          received.current = { binding, sequence: 0, gestureId: '' };
        if (event.sequence <= received.current.sequence) return;
        received.current = { binding, sequence: event.sequence, gestureId: event.gestureId };
        latest.current({
          native,
          binding,
          gestureId: event.gestureId,
          kind: event.kind,
          current: () => current() && received.current.gestureId === event.gestureId,
        });
      }}
    >
      <MessageContextOwnsPress value={handlesTap}>{children}</MessageContextOwnsPress>
    </NativeSource>
  );
}
