import { useEffect, useRef, useState } from 'react';
import { message } from './boundary.ts';
import type { Message } from './messages.ts';

/** Editing never borrows the send composer or clears its independent draft. */
export function MessageEditor({ entry, save, close }: { entry: Message; save: (text: string) => Promise<void>; close: () => void }) {
  const [text, setText] = useState(entry.text);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const field = useRef<HTMLTextAreaElement>(null);
  const dialog = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    field.current?.focus();
    return () => { before?.focus(); };
  }, []);
  return <div className="zm-scrim"><div className="zm-sheet zm-edit-sheet" ref={dialog} role="dialog" aria-modal="true" aria-label="Editar mensagem"
    onKeyDown={(e) => {
      if (e.key === 'Escape' && !busy) { e.stopPropagation(); close(); }
      if (e.key === 'Tab') {
        const controls = [...dialog.current!.querySelectorAll<HTMLElement>('textarea, button:not(:disabled)')];
        const next = controls[(controls.indexOf(document.activeElement as HTMLElement) + (e.shiftKey ? -1 : 1) + controls.length) % controls.length];
        e.preventDefault(); next?.focus();
      }
    }}>
    <h2>Editar mensagem</h2>
    <form onSubmit={(e) => {
      e.preventDefault(); if (busy || !text.trim()) return;
      setBusy(true); setError('');
      void save(text).then(close).catch((e: unknown) => setError(message(e))).finally(() => setBusy(false));
    }}>
      <label>Mensagem<textarea ref={field} value={text} onChange={(e) => setText(e.target.value)} readOnly={busy} /></label>
      {error ? <p className="zm-error" role="alert">{error}</p> : null}
      <div className="zm-edit-buttons"><button type="button" disabled={busy} onClick={close}>Cancelar</button><button type="submit" disabled={busy || !text.trim()}>{busy ? 'Salvando…' : 'Salvar'}</button></div>
    </form>
  </div></div>;
}
