import {
  createContext,
  use,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { createPortal } from 'react-dom';
import { useTheme } from '../theme';
import type { DialogOptions, Dialogs } from './dialog-types';

type Pending = {
  id: number;
  options: DialogOptions;
  information: boolean;
  resolve: (value: boolean) => void;
};
const Context = createContext<Dialogs | null>(null);

function Dialog({
  pending,
  finish,
}: {
  pending: Pending;
  finish: (id: number, accepted: boolean) => void;
}) {
  const { colors } = useTheme();
  const element = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = element.current;
    const previous = document.activeElement;
    dialog?.showModal();
    return () => {
      dialog?.close();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, []);
  const { id, options, information } = pending;
  const complete = (accepted: boolean) => {
    finish(id, accepted);
    element.current?.close();
  };
  return createPortal(
    <dialog
      ref={element}
      aria-labelledby={`zoen-dialog-title-${id}`}
      aria-describedby={`zoen-dialog-detail-${id}`}
      onCancel={(event) => {
        event.preventDefault();
        complete(false);
      }}
      onClose={() => finish(id, false)}
      data-zoen-dialog
      style={{
        color: colors.text,
        background: colors.surface,
        border: `1px solid ${colors.line}`,
        borderRadius: 24,
        padding: 26,
        width: 'min(420px, calc(100vw - 48px))',
        boxSizing: 'border-box',
        boxShadow: '0 24px 80px #0004',
        fontFamily: 'system-ui',
        margin: 'auto',
      }}
    >
      <style>
        {'dialog[data-zoen-dialog]::backdrop { background: #0006; backdrop-filter: blur(4px); }'}
      </style>
      <h2 id={`zoen-dialog-title-${id}`} style={{ fontSize: 22, margin: '0 0 12px' }}>
        {options.title}
      </h2>
      <p
        id={`zoen-dialog-detail-${id}`}
        style={{
          color: colors.secondary,
          fontSize: 16,
          lineHeight: 1.5,
          margin: '0 0 24px',
          whiteSpace: 'pre-wrap',
        }}
      >
        {options.message}
      </p>
      <div style={{ display: 'flex', flexWrap: 'wrap', justifyContent: 'flex-end', gap: 12 }}>
        {!information ? (
          <button
            autoFocus
            onClick={() => complete(false)}
            style={{
              minHeight: 44,
              borderRadius: 14,
              padding: '10px 18px',
              border: `1px solid ${colors.line}`,
              background: colors.surface,
              color: colors.text,
              font: 'inherit',
              cursor: 'pointer',
            }}
          >
            {options.cancelLabel ?? 'Cancelar'}
          </button>
        ) : null}
        <button
          autoFocus={information}
          onClick={() => complete(true)}
          style={{
            minHeight: 44,
            borderRadius: 14,
            padding: '10px 18px',
            border: 0,
            background: options.destructive ? colors.danger : colors.accent,
            color: colors.onAccent,
            font: 'inherit',
            cursor: 'pointer',
          }}
        >
          {options.confirmLabel ?? (information ? 'OK' : 'Continuar')}
        </button>
      </div>
    </dialog>,
    document.body,
  );
}

export function DialogsProvider({ children }: { children: ReactNode }) {
  const queue = useRef<Pending[]>([]);
  const sequence = useRef(0);
  const [pending, setPending] = useState<Pending | null>(null);
  const enqueue = useCallback(
    (options: DialogOptions, information = false) =>
      new Promise<boolean>((resolve) => {
        const request = { id: ++sequence.current, options, information, resolve };
        queue.current.push(request);
        if (queue.current.length === 1) setPending(request);
      }),
    [],
  );
  const finish = useCallback((id: number, accepted: boolean) => {
    if (queue.current[0]?.id !== id) return;
    queue.current.shift()!.resolve(accepted);
    setPending(queue.current[0] ?? null);
  }, []);
  useEffect(
    () => () => {
      for (const request of queue.current.splice(0)) request.resolve(false);
    },
    [],
  );
  const api = useMemo<Dialogs>(
    () => ({
      confirm: enqueue,
      inform: async (title, message) => {
        await enqueue({ title, message }, true);
      },
    }),
    [enqueue],
  );
  return (
    <Context value={api}>
      {children}
      {pending ? <Dialog key={pending.id} pending={pending} finish={finish} /> : null}
    </Context>
  );
}
export function useDialogs(): Dialogs {
  const context = use(Context);
  if (!context) throw new Error('DialogsProvider is missing.');
  return context;
}
