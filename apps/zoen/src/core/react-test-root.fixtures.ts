import * as React from 'react';
import { createRoot } from 'react-dom/client';

/** Real React lifecycle with no host children. Native presentation remains a
 * port; this harness proves effect/subscription behavior, never native pixels. */
export function createReactTestRoot() {
  const globals = globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean };
  const previousWindow = globals.window;
  const previousAct = globals.IS_REACT_ACT_ENVIRONMENT;
  const document = {
    nodeType: 9,
    defaultView: { HTMLIFrameElement: class {} },
    activeElement: null,
    addEventListener() {},
    removeEventListener() {},
  };
  globals.window = document.defaultView as unknown as Window & typeof globalThis;
  globals.IS_REACT_ACT_ENVIRONMENT = true;
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
    onUncaughtError: (error) => errors.push(error),
  });
  return {
    errors,
    async render(element: React.ReactNode) {
      await React.act(async () => root.render(element));
    },
    async act(action: () => unknown) {
      await React.act(async () => {
        await action();
      });
    },
    async close() {
      await React.act(async () => root.unmount());
      globals.window = previousWindow;
      if (previousAct === undefined) delete globals.IS_REACT_ACT_ENVIRONMENT;
      else globals.IS_REACT_ACT_ENVIRONMENT = previousAct;
    },
  };
}
