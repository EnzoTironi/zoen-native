import { t } from '../i18n/index.ts';
import { forwardRef, useLayoutEffect, useImperativeHandle, useRef } from 'react';
import type { MiniappFrameHandle, MiniappFrameProps } from './frame-types';
import { createRuntimeHost } from './runtime-browser.ts';

export const MiniappFrame = forwardRef<MiniappFrameHandle, MiniappFrameProps>(function MiniappFrame(
  { html, onMessage, onError, height, title, blocked, runtimePackage },
  ref,
) {
  const frame = useRef<HTMLIFrameElement>(null);
  const runtime = useRef<ReturnType<typeof createRuntimeHost> | null>(null);
  const callbacks = useRef({ onMessage, onError });
  useLayoutEffect(() => {
    callbacks.current = { onMessage, onError };
  }, [onMessage, onError]);
  useImperativeHandle(
    ref,
    () => ({
      send: (message) => frame.current?.contentWindow?.postMessage(message, '*'),
      dispose: () => {
        runtime.current?.dispose();
        if (frame.current) frame.current.srcdoc = '';
      },
    }),
    [],
  );
  useLayoutEffect(() => {
    let active = true;
    const owner = runtimePackage
      ? createRuntimeHost(
          () => frame.current?.contentWindow ?? null,
          runtimePackage,
          () => {
            if (!active) throw Error('App runtime retired');
          },
        )
      : null;
    runtime.current = owner;
    const receive = (event: MessageEvent<unknown>) => {
      // allow-scripts without allow-same-origin gives each view a fresh opaque origin.
      if (event.source !== frame.current?.contentWindow || event.origin !== 'null') return;
      if (owner?.receive(event)) return;
      try {
        callbacks.current.onMessage(
          typeof event.data === 'string' ? event.data : JSON.stringify(event.data),
        );
      } catch {
        callbacks.current.onError(t('miniapps.theInterfaceSentAnInvalidMessage'));
      }
    };
    window.addEventListener('message', receive);
    return () => {
      active = false;
      owner?.dispose();
      if (runtime.current === owner) runtime.current = null;
      window.removeEventListener('message', receive);
    };
  }, [html, runtimePackage]);
  return (
    <iframe
      ref={frame}
      srcDoc={html}
      title={title}
      sandbox="allow-scripts"
      allow="camera 'none'; microphone 'none'; geolocation 'none'; clipboard-read 'none'; clipboard-write 'none'"
      referrerPolicy="no-referrer"
      {...{ credentialless: '' }}
      inert={blocked}
      tabIndex={blocked ? -1 : 0}
      onError={() => onError(t('miniapps.theMiniappInterfaceCouldNotBeOpened'))}
      style={{
        height,
        minHeight: 400,
        width: '100%',
        border: 0,
        background: 'transparent',
        pointerEvents: blocked ? 'none' : 'auto',
      }}
    />
  );
});
