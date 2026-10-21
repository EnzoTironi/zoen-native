import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as React from 'react';
import { createRoot } from 'react-dom/client';
import { AttachmentKeyboardSession } from '../media/attachment-keyboard-session.ts';
import type { MediaActionsProps } from '../media/media-actions.types.ts';

const require = createRequire(import.meta.url);
const code = (
  await build({
    stdin: {
      contents: readFileSync(
        process.env.ZOEN_MEDIA_ACTIONS_SOURCE ??
          new URL('../media/media-actions.ios.tsx', import.meta.url),
        'utf8',
      ),
      loader: 'tsx',
    },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    write: false,
    plugins: [
      {
        name: 'native-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
        },
      },
    ],
  })
).outputFiles[0]!.text;

type ButtonProps = { onPress(): void; disabled: boolean; name: string };
/** Actual React reconciler, effects and external-store subscriptions. The native
 * button renders no DOM children; the empty root supplies only event metadata. */
function fixture() {
  const globals = globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean };
  const previousWindow = globals.window,
    previousAct = globals.IS_REACT_ACT_ENVIRONMENT;
  const document = {
    nodeType: 9,
    defaultView: { HTMLIFrameElement: class {} },
    activeElement: null,
    addEventListener() {},
    removeEventListener() {},
  };
  globals.IS_REACT_ACT_ENVIRONMENT = true;
  globals.window = document.defaultView as unknown as Window & typeof globalThis;
  const container = {
    nodeType: 1,
    nodeName: 'DIV',
    tagName: 'DIV',
    namespaceURI: 'http://www.w3.org/1999/xhtml',
    ownerDocument: document,
    addEventListener() {},
    removeEventListener() {},
  };
  const errors: unknown[] = [];
  const root = createRoot(container as unknown as Element, {
    onUncaughtError(error) {
      errors.push(error);
    },
  });
  let next = 0,
    renders = 0,
    registrations = 0,
    activeRegistrations = 0;
  let button: ButtonProps | undefined;
  const session = new AttachmentKeyboardSession('chat', true, () => `id-${++next}`);
  session.bind({ tag: 7, context: 'chat:normal', label: 'Message', placeholder: '' });
  const register = session.register.bind(session);
  session.register = (...args) => {
    registrations++;
    activeRegistrations++;
    const dispose = register(...args);
    return () => {
      activeRegistrations--;
      dispose();
    };
  };
  const ports: Record<string, unknown> = {
    react: React,
    'react/jsx-runtime': require('react/jsx-runtime'),
    './attachment-keyboard-context': { useAttachmentKeyboardSession: () => session },
    '../i18n/index.ts': { t: (value: string) => value },
    '../components/composer-action': {
      ComposerAction: (props: ButtonProps) => {
        button = props;
        return null;
      },
    },
    '../theme': { useTheme: () => ({ scheme: 'dark', colors: {} }) },
    '@expo/ui': {},
    '@expo/ui/swift-ui': {},
    '@expo/ui/swift-ui/modifiers': {},
    'expo-glass-effect': {},
  };
  const module = { exports: {} as { MediaActions: React.ComponentType<MediaActionsProps> } };
  runInNewContext(code, {
    module,
    exports: module.exports,
    require(path: string) {
      assert.ok(path in ports, path);
      return ports[path];
    },
  });
  function Composer({ disabled, hasVoiceEditor, onChoose }: MediaActionsProps) {
    // A subscriber higher in the composer may rerender with a new onChoose
    // closure. Registration must not turn store observation into a feedback loop.
    React.useSyncExternalStore(session.subscribe, session.getSnapshot, session.getSnapshot);
    renders++;
    return React.createElement(module.exports.MediaActions, {
      disabled,
      hasVoiceEditor,
      onChoose: (kind) => onChoose(kind),
    });
  }
  return {
    session,
    errors,
    get button() {
      assert.ok(button);
      return button;
    },
    get registrations() {
      return registrations;
    },
    get activeRegistrations() {
      return activeRegistrations;
    },
    get renders() {
      return renders;
    },
    async render(props: MediaActionsProps) {
      await React.act(async () => root.render(React.createElement(Composer, props)));
    },
    async act(action: () => void) {
      await React.act(async () => action());
    },
    async close() {
      await React.act(async () => root.unmount());
      globals.window = previousWindow;
      if (previousAct === undefined) delete globals.IS_REACT_ACT_ENVIRONMENT;
      else globals.IS_REACT_ACT_ENVIRONMENT = previousAct;
    },
  };
}

test('actual React settles registration with a pending document and fresh composer callbacks', async () => {
  const f = fixture();
  try {
    const chosen: string[] = [];
    let finish!: () => void;
    const picker = new Promise<void>((resolve) => {
      finish = resolve;
    });
    const props = {
      disabled: false,
      // A pending canonical PDF disables voice import but still admits Add.
      hasVoiceEditor: false,
      onChoose: async (kind: string) => {
        chosen.push(kind);
        await picker;
      },
    };
    await f.render(props);
    assert.deepEqual(
      f.errors,
      [],
      'React must not hit its nested update limit during registration',
    );
    assert.equal(f.registrations, 1);
    assert.equal(f.activeRegistrations, 1);
    await f.act(() => f.button.onPress());
    assert.deepEqual(chosen, ['photo']);
    const id = f.session.getSnapshot().presentation!.id;
    assert.equal(f.session.admitPicker(id), true);
    await f.render({ ...props, disabled: true });
    assert.equal(f.button.name, 'xmark');
    assert.equal(f.button.disabled, false, 'The admitted picker still offers Close');
    await f.act(() => f.button.onPress());
    await f.act(() => {
      f.session.closed(id);
      finish();
    });
    await f.render(props);
    assert.equal(f.button.name, 'plus');
    assert.equal(f.button.disabled, false);
    assert.deepEqual(f.errors, []);
    assert.equal(f.activeRegistrations, 1);
    assert.ok(f.renders < 20, `Composer must settle instead of cycling (${f.renders} renders)`);
  } finally {
    await f.close();
  }
  assert.equal(f.activeRegistrations, 0);
});

test('actual React registration keeps the latest committed callback and retires on disable/unmount', async () => {
  const f = fixture(),
    chosen: string[] = [];
  try {
    const props = {
      disabled: false,
      hasVoiceEditor: false,
      onChoose: async () => {
        chosen.push('old draft');
      },
    };
    await f.render(props);
    const plus = f.button;
    await f.render({
      ...props,
      onChoose: async () => {
        chosen.push('current draft');
      },
    });
    assert.deepEqual(f.errors, []);
    assert.equal(f.registrations, 1, 'Callback updates keep the registration owner');
    await f.act(() => plus.onPress());
    assert.deepEqual(chosen, ['current draft']);
    await f.render({ ...props, disabled: true });
    await f.act(() => plus.onPress());
    assert.deepEqual(chosen, ['current draft']);
  } finally {
    await f.close();
  }
  f.button.onPress();
  assert.deepEqual(chosen, ['current draft']);
  assert.equal(f.activeRegistrations, 0);
});

test('actual React Files handoff waits both close barriers and dispatches only the current enabled callback', async () => {
  const f = fixture(),
    chosen: string[] = [];
  let finish!: () => void;
  const picker = new Promise<void>((resolve) => {
    finish = resolve;
  });
  const props = {
    disabled: false,
    hasVoiceEditor: false,
    onChoose: async (kind: string) => {
      chosen.push(`first:${kind}`);
      await picker;
    },
  };
  try {
    await f.render(props);
    await f.act(() => f.button.onPress());
    const id = f.session.getSnapshot().presentation!.id;
    assert.equal(f.session.admitPicker(id), true);
    await f.act(() => {
      f.session.handoff(id, 'document');
      f.session.closed(id);
    });
    assert.deepEqual(chosen, ['first:photo'], 'Native removal alone does not dispatch Files');
    await f.render({ ...props, disabled: true });
    await f.act(() => finish());
    assert.deepEqual(
      chosen,
      ['first:photo'],
      'A disabled composer cannot dispatch the pending handoff',
    );
    await f.render({
      ...props,
      onChoose: async (kind: string) => {
        chosen.push(`current:${kind}`);
      },
    });
    assert.deepEqual(f.errors, []);
    assert.deepEqual(chosen, ['first:photo', 'current:document']);
    assert.equal(f.session.owns(id), false);
    await f.act(() => {
      f.session.closed(id);
    });
    assert.deepEqual(chosen, ['first:photo', 'current:document']);
    assert.equal(f.activeRegistrations, 1);
    assert.ok(f.renders < 20);
  } finally {
    await f.close();
  }
});
