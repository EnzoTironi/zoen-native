import { t } from '../i18n/index.ts';
import { forwardRef, useEffect, useImperativeHandle, useRef } from 'react';
import type { MiniappFrameHandle, MiniappFrameProps } from './frame-types';

export const MiniappFrame = forwardRef<MiniappFrameHandle, MiniappFrameProps>(function MiniappFrame(
  { html, onMessage, onError, height, title, blocked },
  ref,
) {
  const frame = useRef<HTMLIFrameElement>(null);
  useImperativeHandle(
    ref,
    () => ({
      send: (message) => frame.current?.contentWindow?.postMessage(message, '*'),
      dispose: () => {
        if (frame.current) frame.current.srcdoc = '';
      },
    }),
    [],
  );
  useEffect(() => {
    const receive = (event: MessageEvent<unknown>) => {
      // allow-scripts without allow-same-origin gives each view a fresh opaque origin.
      if (event.source !== frame.current?.contentWindow || event.origin !== 'null') return;
      try {
        onMessage(typeof event.data === 'string' ? event.data : JSON.stringify(event.data));
      } catch {
        onError(t('miniapps.theInterfaceSentAnInvalidMessage'));
      }
    };
    window.addEventListener('message', receive);
    return () => window.removeEventListener('message', receive);
  }, [onMessage, onError]);
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
