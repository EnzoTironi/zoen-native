import { useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { useTheme } from '../theme';
import { t } from '../i18n';
import type { PendingDialogChoice } from './dialog-choice-queue';

export function DialogChoices({
  pending,
  finish,
}: {
  pending: PendingDialogChoice;
  finish(id: number, value: string | null): void;
}) {
  const { colors } = useTheme();
  const element = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const previous = document.activeElement;
    const dialog = element.current;
    dialog?.showModal();
    return () => {
      dialog?.close();
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, []);
  return createPortal(
    <dialog
      ref={element}
      aria-labelledby={`choice-title-${pending.id}`}
      onCancel={(event) => {
        event.preventDefault();
        finish(pending.id, null);
      }}
      onClose={() => finish(pending.id, null)}
      style={{
        color: colors.text,
        background: colors.surface,
        border: `1px solid ${colors.line}`,
        borderRadius: 24,
        padding: 24,
        width: 'min(420px, calc(100vw - 48px))',
        margin: 'auto',
      }}
    >
      <h2 id={`choice-title-${pending.id}`} style={{ fontSize: 22, margin: '0 0 16px' }}>
        {pending.options.title}
      </h2>
      <div style={{ display: 'grid', gap: 8 }}>
        {pending.options.choices.map((choice) => (
          <button
            key={choice.id}
            onClick={() => finish(pending.id, choice.id)}
            style={{
              minHeight: 44,
              border: 0,
              borderRadius: 12,
              background: colors.raised,
              color: choice.destructive ? colors.danger : colors.text,
              font: 'inherit',
              cursor: 'pointer',
            }}
          >
            {choice.label}
          </button>
        ))}
        <button
          autoFocus
          onClick={() => finish(pending.id, null)}
          style={{
            minHeight: 44,
            border: 0,
            background: 'transparent',
            color: colors.text,
            font: 'inherit',
            cursor: 'pointer',
          }}
        >
          {t('common.cancel')}
        </button>
      </div>
    </dialog>,
    document.body,
  );
}
