import { useRef, useState } from 'react';
import { usePreventRemove } from 'expo-router';
import { useDialogs } from '../components/dialogs';

/** SDK 58 also protects browser reloads and closing the tab. */
export function useDirtyEditor(dirty: boolean, busy: boolean) {
  const dialogs = useDialogs();
  const [leaving, setLeaving] = useState(false);
  const confirming = useRef(false);
  const disable = usePreventRemove(!leaving && (dirty || busy), ({ repeat }) => {
    if (confirming.current) return;
    if (busy) {
      void dialogs.inform('Guardando alterações', 'Espere esta operação terminar antes de sair.');
      return;
    }
    confirming.current = true;
    void dialogs
      .confirm({
        title: 'Sair sem guardar?',
        message: 'As alterações deste editor ainda não foram guardadas no rascunho.',
        confirmLabel: 'Descartar alterações',
        cancelLabel: 'Continuar editando',
        destructive: true,
      })
      .then((accepted) => {
        if (accepted) {
          setLeaving(true);
          repeat();
        }
      })
      .finally(() => {
        confirming.current = false;
      });
  });
  return () => {
    setLeaving(true);
    disable();
  };
}
