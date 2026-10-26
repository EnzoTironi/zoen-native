import * as React from 'react';
import { createRequire } from 'node:module';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { loadEditorAgent } from './editor-agent-compile.fixtures.ts';
import { telegramPresentation } from '../theme/telegram.ts';
import * as presentation from './telegram-composer-presentation.ts';
import * as motion from './telegram-composer-motion.ts';
import { conversationQueryFits } from './conversation-search.ts';
import { composerOwnerFixture } from './composer-owner.fixtures.ts';
import { homeOwnerFor } from '../home/access.ts';

type Props = Record<string, any>;
export type Frame = {
  width: number;
  height: number;
  left: number;
  right: number;
  top: number;
  bottom: number;
};
const require = createRequire(import.meta.url);
const flatten = (style: any): any =>
  Array.isArray(style) ? Object.assign({}, ...style.map(flatten)) : (style ?? {});
/** Actual Expo/Compiler composer, iOS actions and viewport. Native Views,
 * keyboard shared values, text focus and measured bounds are explicit ports. */
export async function composerSafeAreaFixture(searching = true, noResults = false) {
  const client = composerOwnerFixture();
  const root = createReactTestRoot(),
    controls = new Map<string, Props>();
  const Context = React.createContext<Frame>({
    width: 402,
    height: 874,
    left: 0,
    right: 0,
    top: 62,
    bottom: 34,
  });
  const ActionContext = React.createContext({ active: true, imageStyle: {} });
  let frame = { width: 402, height: 874, left: 0, right: 0, top: 62, bottom: 34 };
  let setFrame: ((value: Frame) => void) | null = null;
  let mounts = 0,
    unmounts = 0;
  const heights: number[] = [],
    actions: string[] = [],
    input = { focused: false };
  const clock = { height: { value: -220 }, progress: { value: 1 } };
  const useSafe = () => React.useContext(Context);
  function View(props: Props) {
    React.useLayoutEffect(() => {
      const id = props.testID ?? (props.onLayout ? 'native-composer-layout' : null);
      if (id) controls.set(id, props);
      return () => {
        if (id && controls.get(id) === props) controls.delete(id);
      };
    }, [props]);
    return props.children;
  }
  function Input(props: Props) {
    React.useEffect(() => {
      mounts++;
      return () => {
        unmounts++;
        input.focused = false;
      };
    }, []);
    React.useLayoutEffect(() => {
      controls.set('message-input', props);
    }, [props]);
    return null;
  }
  const shared = (initial: any) => ({
    value: initial,
    get() {
      return this.value;
    },
    set(value: any) {
      this.value = value;
    },
  });
  const useSharedValue = (initial: any) => React.useState(() => shared(initial))[0];
  const theme = {
    colors: { text: '#fff', secondary: '#aaa', accent: '#08f', danger: '#f00' },
    scheme: 'dark',
    reducedMotion: false,
    presentation: telegramPresentation('night'),
  };
  const children = ({ children }: Props) => children;
  let Action!: (props: Props) => React.ReactNode;
  const common: Record<string, any> = {
    react: React,
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react-native': {
      View,
      Text: () => null,
      Pressable: View,
      useWindowDimensions: useSafe,
      StyleSheet: { flatten },
    },
    'react-native-safe-area-context': { useSafeAreaInsets: useSafe },
    'react-native-reanimated': {
      __esModule: true,
      default: { View },
      useAnimatedStyle: (read: () => any) => read(),
      useSharedValue,
      useDerivedValue: (read: () => any) => ({
        get value() {
          return read();
        },
      }),
      withTiming: (value: any) => value,
      Easing: { bezier: () => null },
      ReduceMotion: { System: 'system' },
    },
    'react-native-keyboard-controller': {
      useReanimatedKeyboardAnimation: () => clock,
      KeyboardChatScrollView: View,
      KeyboardAvoidingView: View,
    },
    './keyboard-motion': { useKeyboardMotion: () => clock },
    '../theme': { useTheme: () => theme },
    '../i18n/index.ts': { t: (key: string) => key },
    '../core/telegram-composer-presentation': presentation,
    '../core/telegram-composer-motion': motion,
    '../core/conversation-search': { conversationQueryFits },
    '../core/provider': { useClient: () => client },
    '../home/access': { homeOwnerFor },
    './composer-action-motion': {
      useComposerActionMotion: () => React.useContext(ActionContext),
      composerFrameTransition: () => undefined,
      ComposerMotionSlot: (props: Props) =>
        React.createElement(
          ActionContext.Provider,
          { value: { active: props.active, imageStyle: {} } },
          React.createElement(View, props, props.children),
        ),
    },
    './composer-control-surface': { ComposerControlSurface: children, ComposerGlyph: () => null },
    './composer-glass': { ComposerGlassContainer: View, ComposerGlassSurface: View },
    './composer-text-field': { ComposerTextField: Input },
    './conversation-composer-access': { ConversationComposerAccess: children },
    '../media/attachment-keyboard-context': {
      AttachmentKeyboardProvider: (props: Props) =>
        React.cloneElement(props.children, { testID: 'message-composer-safe-area' }),
      useAttachmentKeyboard: () => ({ kind: 'system', toggleEmoji: () => actions.push('emoji') }),
    },
    '../media/MediaComposer': {
      MediaComposer: (props: Props) =>
        props.renderControls({
          attachment: React.createElement(Action, {
            name: 'plus',
            testID: 'add-attachment',
            label: 'Add',
            onPress: () => actions.push('add'),
          }),
          voice: React.createElement(Action, {
            name: 'mic',
            testID: 'voice',
            label: 'Voice',
            onPress: () => actions.push('voice'),
          }),
        }),
    },
    '../media/attachment-reply-context': { AttachmentReplyContext: children },
    './message-context': {
      MessageContext: (props: Props) =>
        React.createElement(Action, {
          name: 'xmark',
          testID: 'cancel-reply',
          label: 'Cancel reply',
          onPress: props.onCancel,
        }),
    },
    './primitives': { NativeButton: View },
  };
  Action = loadEditorAgent('../components/composer-action.ios.tsx', common).ComposerAction;
  common['./composer-action'] = { ComposerAction: Action };
  const Composer = loadEditorAgent('../components/message-composer.tsx', common).MessageComposer;
  const Viewport = loadEditorAgent(
    '../components/conversation-viewport.tsx',
    common,
  ).ConversationViewport;
  function App() {
    const [metrics, updateFrame] = React.useState(frame),
      [text, updateText] = React.useState(searching ? 'QA345' : 'Unsent draft');
    React.useLayoutEffect(() => {
      setFrame = updateFrame;
      return () => {
        setFrame = null;
      };
    }, [updateFrame]);
    const props = {
      spaceId: 'same-space',
      value: text,
      sending: false,
      onChangeText: updateText,
      onBlur: () => {},
      onSend: () => actions.push('send'),
      context: searching
        ? undefined
        : { entry: { id: 'reply' }, onCancel: () => actions.push('cancel-reply') },
      search: searching
        ? {
            state: {
              active: true,
              query: text,
              index: noResults ? -1 : 1,
              hits: noResults ? [] : [{}, {}, {}],
              completed: true,
              loading: false,
              error: null,
              cursor: null,
            },
            status: 'Result',
            onClose: () => actions.push('close'),
            onMove: (direction: string) => actions.push(direction),
            onAsk: () => actions.push('ask'),
            onSearch: () => actions.push('search'),
          }
        : undefined,
    };
    const viewportProps = {
      composer: React.createElement(Composer, props),
      onComposerLayout: (height: number) => heights.push(height),
      children: () => null,
    };
    return React.createElement(
      Context.Provider,
      { value: metrics },
      React.createElement(
        View,
        { testID: 'wallpaper', style: { position: 'absolute', left: 0, right: 0 } },
        React.createElement(Viewport, viewportProps),
      ),
    );
  }
  return {
    controls,
    actions,
    clock,
    input,
    heights,
    frame: () => frame,
    rotate: async (next: Frame) => {
      frame = next;
      await root.act(() => {
        if (!setFrame) throw Error('Composer is not mounted');
        setFrame(next);
      });
    },
    focus: () => {
      input.focused = true;
      controls.get('message-input')!.onFocus?.();
    },
    mounts: () => ({ mounts, unmounts }),
    style: (id: string) => {
      const raw = controls.get(id)!.style;
      const style = flatten(typeof raw === 'function' ? raw({ pressed: false }) : raw);
      return {
        ...style,
        paddingLeft: style.paddingLeft ?? style.paddingHorizontal,
        paddingRight: style.paddingRight ?? style.paddingHorizontal,
      };
    },
    control: (id: string) => controls.get(id)!,
    render: () => root.render(React.createElement(App)),
    act: root.act,
    errors: root.errors,
    close: root.close,
  };
}
