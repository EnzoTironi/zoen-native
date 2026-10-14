import { createElement as h, useEffect, useRef } from 'react';

export type ForgetCopy = {
  open: string; title: string; body: string; phone: string; keep: string; forget: string;
};

export function forgetCopy(language: string, count: number): ForgetCopy {
  if (language.toLowerCase().startsWith('pt')) {
    const what = count === 0 ? 'Nenhuma conversa está guardada aqui.' :
      count === 1 ? 'A conversa guardada aqui vai sair deste navegador.' :
        `As ${count} conversas guardadas aqui vão sair deste navegador.`;
    return {
      open: 'Esquecer este navegador', title: 'Esquecer este navegador?', body: what,
      phone: 'No celular tudo continua. Para tirar este navegador de vez, abra Aparelhos no celular e remova-o.',
      keep: 'Manter', forget: 'Esquecer',
    };
  }
  const what = count === 0 ? 'No conversations are stored here.' :
    count === 1 ? 'The conversation stored here will leave this browser.' :
      `The ${count} conversations stored here will leave this browser.`;
  return {
    open: 'Forget this browser', title: 'Forget this browser?', body: what,
    phone: 'Everything stays on your phone. To remove this browser for good, open Devices on your phone and remove it.',
    keep: 'Keep', forget: 'Forget',
  };
}

/** A modal that says what leaves this browser before anything is erased. Keep is focused first. */
export function ForgetDialog({ language, count, open, keep, forget }: {
  language: string; count: number; open: boolean; keep: () => void; forget: () => void;
}) {
  const copy = forgetCopy(language, count);
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const node = dialog.current;
    if (!node) return;
    if (open && !node.open) node.showModal();
    if (!open && node.open) node.close();
  }, [open]);
  return h('dialog', {
    ref: dialog, className: 'zw-forget', 'aria-labelledby': 'zw-forget-title', 'aria-describedby': 'zw-forget-body',
    onCancel: (event: Event) => { event.preventDefault(); keep(); },
    onClick: (event: MouseEvent) => { if (event.target === dialog.current) keep(); },
  },
  h('div', { className: 'zw-forget-card' },
    h('h2', { id: 'zw-forget-title' }, copy.title),
    h('p', { id: 'zw-forget-body' }, h('strong', null, copy.body), ' ', copy.phone),
    h('div', { className: 'zw-forget-actions' },
      h('button', { type: 'button', className: 'zw-secondary', autoFocus: true, onClick: keep }, copy.keep),
      h('button', { type: 'button', className: 'zw-danger', onClick: forget }, copy.forget))));
}
